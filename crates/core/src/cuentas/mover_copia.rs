//! Copia de una carpeta de cuenta a otro sistema de ficheros y verificación de que la
//! copia es completa, antes de que `mover_carpeta` borre el origen.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::config::RutasCuenta;
use crate::git;
use crate::idioma::TextoExterno;

use super::error::ErrorCuentas;
use super::mover_textos::{ProblemaCopia, ProgresoMover};

/// Cada cuánto como máximo se avisa del progreso de la copia (el último aviso siempre
/// sale).
const INTERVALO_PROGRESO: Duration = Duration::from_millis(250);

/// Ficheros cuyo contenido se compara con SHA-256, relativos a la carpeta de la cuenta.
const FICHEROS_CRITICOS: [&str; 2] = ["gitea/data/gitea.db", "gitmereba.toml"];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tipo {
    Directorio,
    Fichero { tamano: u64 },
    Enlace { destino: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entrada {
    relativa: PathBuf,
    tipo: Tipo,
    /// Bits de permiso (`0o7777`). Sin significado para un enlace.
    modo: u32,
}

fn io(error: &std::io::Error) -> ErrorCuentas {
    ErrorCuentas::Io(error.to_string())
}

fn copia(problema: ProblemaCopia) -> ErrorCuentas {
    ErrorCuentas::Copia(problema)
}

/// Recorre `raiz` sin seguir enlaces simbólicos y devuelve todo lo que contiene (la raíz
/// no), en preorden y con los hijos de cada directorio ordenados por nombre. Un tipo
/// especial (FIFO, dispositivo, socket) es un error.
fn listar(raiz: &Path) -> Result<Vec<Entrada>, ErrorCuentas> {
    let mut resultado = Vec::new();
    let mut pendientes = vec![PathBuf::new()];
    while let Some(relativa) = pendientes.pop() {
        let mut hijos = Vec::new();
        for entrada in std::fs::read_dir(raiz.join(&relativa)).map_err(|e| io(&e))? {
            hijos.push(entrada.map_err(|e| io(&e))?.file_name());
        }
        hijos.sort();
        let mut directorios = Vec::new();
        for nombre in hijos {
            let hija = relativa.join(&nombre);
            let ruta = raiz.join(&hija);
            let metadatos = std::fs::symlink_metadata(&ruta).map_err(|e| io(&e))?;
            let tipo_fs = metadatos.file_type();
            let modo = metadatos.permissions().mode() & 0o7777;
            let tipo = if tipo_fs.is_dir() {
                directorios.push(hija.clone());
                Tipo::Directorio
            } else if tipo_fs.is_file() {
                Tipo::Fichero {
                    tamano: metadatos.len(),
                }
            } else if tipo_fs.is_symlink() {
                Tipo::Enlace {
                    destino: std::fs::read_link(&ruta).map_err(|e| io(&e))?,
                }
            } else {
                return Err(copia(ProblemaCopia::TipoEspecial { ruta }));
            };
            resultado.push(Entrada {
                relativa: hija,
                tipo,
                modo,
            });
        }
        // Preorden: el primer directorio hijo se recorre antes que el siguiente.
        pendientes.extend(directorios.into_iter().rev());
    }
    // El orden de visita de arriba no es el preorden estricto (primero todos los hijos
    // de un directorio): se ordena por ruta, que sí lo es y es estable.
    resultado.sort_by(|a, b| a.relativa.cmp(&b.relativa));
    Ok(resultado)
}

/// Copia recursivamente `origen` en `destino` (que no debe existir), conservando los
/// modos y los enlaces simbólicos (sin seguirlos). Todo se ha comprobado antes de crear
/// nada: un tipo especial en el origen no deja una copia a medias. Cada fichero y cada
/// directorio se sincronizan a disco, porque después se borra el origen.
///
/// Es código bloqueante: se llama desde el hilo de la operación, no desde un ejecutor
/// compartido.
#[cfg(test)]
pub(super) fn copiar_arbol(
    origen: &Path,
    destino: &Path,
    al_progresar: &mut dyn FnMut(ProgresoMover),
) -> Result<(), ErrorCuentas> {
    crear_raiz_copia(destino)?;
    copiar_en(origen, destino, al_progresar)
}

/// Crea, sin pisar nada, el directorio raíz de la copia (0700 para poder escribir en
/// él; el modo real se aplica al final). Si ya existe, falla: quien llama solo puede
/// borrar en el deshacer lo que esta función creó.
pub(super) fn crear_raiz_copia(destino: &Path) -> Result<(), ErrorCuentas> {
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(destino)
        .map_err(|e| io(&e))
}

/// Copia el contenido de `origen` dentro de `destino`, que ya existe y está vacío
/// ([`crear_raiz_copia`]).
pub(super) fn copiar_en(
    origen: &Path,
    destino: &Path,
    al_progresar: &mut dyn FnMut(ProgresoMover),
) -> Result<(), ErrorCuentas> {
    let entradas = listar(origen)?;
    let ficheros_total = entradas
        .iter()
        .filter(|e| matches!(e.tipo, Tipo::Fichero { .. }))
        .count() as u64;
    let bytes_total: u64 = entradas
        .iter()
        .map(|e| match e.tipo {
            Tipo::Fichero { tamano } => tamano,
            _ => 0,
        })
        .sum();
    let modo_raiz = std::fs::symlink_metadata(origen)
        .map_err(|e| io(&e))?
        .permissions()
        .mode()
        & 0o7777;

    let mut ficheros = 0u64;
    let mut bytes = 0u64;
    let mut ultimo_aviso = Instant::now();
    al_progresar(ProgresoMover::Copiando {
        ficheros,
        ficheros_total,
        bytes,
        bytes_total,
    });

    // Los directorios se crean 0700 para poder escribir en ellos; el modo real se aplica
    // al final, de los hijos a los padres.
    for entrada in &entradas {
        let origen_entrada = origen.join(&entrada.relativa);
        let destino_entrada = destino.join(&entrada.relativa);
        match &entrada.tipo {
            Tipo::Directorio => std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&destino_entrada)
                .map_err(|e| io(&e))?,
            Tipo::Fichero { tamano } => {
                std::fs::copy(&origen_entrada, &destino_entrada).map_err(|e| io(&e))?;
                std::fs::File::open(&destino_entrada)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| io(&e))?;
                ficheros += 1;
                bytes += tamano;
                if ultimo_aviso.elapsed() >= INTERVALO_PROGRESO {
                    ultimo_aviso = Instant::now();
                    al_progresar(ProgresoMover::Copiando {
                        ficheros,
                        ficheros_total,
                        bytes,
                        bytes_total,
                    });
                }
            }
            Tipo::Enlace { destino: objetivo } => {
                symlink(objetivo, &destino_entrada).map_err(|e| io(&e))?;
            }
        }
    }
    for entrada in entradas.iter().rev() {
        if entrada.tipo == Tipo::Directorio {
            let ruta = destino.join(&entrada.relativa);
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(entrada.modo))
                .map_err(|e| io(&e))?;
            std::fs::File::open(&ruta)
                .and_then(|d| d.sync_all())
                .map_err(|e| io(&e))?;
        }
    }
    std::fs::set_permissions(destino, std::fs::Permissions::from_mode(modo_raiz))
        .map_err(|e| io(&e))?;
    std::fs::File::open(destino)
        .and_then(|d| d.sync_all())
        .map_err(|e| io(&e))?;

    al_progresar(ProgresoMover::Copiando {
        ficheros,
        ficheros_total,
        bytes,
        bytes_total,
    });
    Ok(())
}

/// Compara los dos árboles: mismas rutas, tipos, tamaños, modos y destinos de enlace.
pub(super) fn comparar_arboles(origen: &Path, copia_: &Path) -> Result<(), ErrorCuentas> {
    let en_origen: BTreeMap<PathBuf, Entrada> = listar(origen)?
        .into_iter()
        .map(|e| (e.relativa.clone(), e))
        .collect();
    let mut en_copia: BTreeMap<PathBuf, Entrada> = listar(copia_)?
        .into_iter()
        .map(|e| (e.relativa.clone(), e))
        .collect();
    for (relativa, entrada) in &en_origen {
        let Some(otra) = en_copia.remove(relativa) else {
            return Err(copia(ProblemaCopia::Falta {
                ruta: relativa.clone(),
            }));
        };
        let mismo_modo = matches!(entrada.tipo, Tipo::Enlace { .. }) || entrada.modo == otra.modo;
        if entrada.tipo != otra.tipo || !mismo_modo {
            return Err(copia(ProblemaCopia::Distinta {
                ruta: relativa.clone(),
            }));
        }
    }
    if let Some(sobrante) = en_copia.into_keys().next() {
        return Err(copia(ProblemaCopia::Sobra { ruta: sobrante }));
    }
    // Las raíces tienen que tener también el mismo modo.
    let modo_de =
        |ruta: &Path| std::fs::symlink_metadata(ruta).map(|m| m.permissions().mode() & 0o7777);
    if modo_de(origen).map_err(|e| io(&e))? != modo_de(copia_).map_err(|e| io(&e))? {
        return Err(copia(ProblemaCopia::Distinta {
            ruta: PathBuf::new(),
        }));
    }
    Ok(())
}

fn sha256_de(ruta: &Path) -> Result<[u8; 32], ErrorCuentas> {
    let mut fichero = std::fs::File::open(ruta).map_err(|e| io(&e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let leidos = fichero.read(&mut buffer).map_err(|e| io(&e))?;
        if leidos == 0 {
            break;
        }
        hasher.update(&buffer[..leidos]);
    }
    Ok(hasher.finalize().into())
}

/// SHA-256 de `gitea/data/gitea.db` y de `gitmereba.toml`, origen contra copia.
pub(super) fn comparar_hashes(origen: &Path, copia_: &Path) -> Result<(), ErrorCuentas> {
    for relativa in FICHEROS_CRITICOS {
        let en_origen = origen.join(relativa);
        if !en_origen.exists() {
            continue;
        }
        let coincide = sha256_de(&en_origen)? == sha256_de(&copia_.join(relativa))?;
        if !coincide {
            return Err(copia(ProblemaCopia::HashDistinto {
                ruta: PathBuf::from(relativa),
            }));
        }
    }
    Ok(())
}

/// Repos bare `<raiz>/<dueno>/<nombre>.git` bajo `raiz`, en orden.
fn repos_bare(raiz: &Path) -> Result<Vec<PathBuf>, ErrorCuentas> {
    let mut repos = Vec::new();
    if !raiz.is_dir() {
        return Ok(repos);
    }
    let mut duenos: Vec<PathBuf> = std::fs::read_dir(raiz)
        .map_err(|e| io(&e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    duenos.sort();
    for dueno in duenos {
        let es_directorio = std::fs::symlink_metadata(&dueno)
            .map(|m| m.file_type().is_dir())
            .unwrap_or(false);
        if !es_directorio {
            continue;
        }
        let mut hijos: Vec<PathBuf> = std::fs::read_dir(&dueno)
            .map_err(|e| io(&e))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|ruta| ruta.extension().is_some_and(|ext| ext == "git"))
            .collect();
        hijos.sort();
        repos.extend(hijos.into_iter().filter(|ruta| {
            std::fs::symlink_metadata(ruta)
                .map(|m| m.file_type().is_dir())
                .unwrap_or(false)
        }));
    }
    Ok(repos)
}

/// `git fsck` de cada repositorio bare de la copia (repositorios de Gitea y snapshots).
pub(super) async fn fsck_repositorios(
    copia_: &RutasCuenta,
    al_progresar: &mut dyn FnMut(ProgresoMover),
) -> Result<(), ErrorCuentas> {
    let mut repos = repos_bare(&copia_.gitea_repositorios())?;
    repos.extend(repos_bare(&copia_.snapshots())?);
    let total = repos.len() as u64;
    al_progresar(ProgresoMover::Verificando { hechos: 0, total });
    for (indice, repo) in repos.iter().enumerate() {
        git::fsck(repo).await.map_err(|error| {
            copia(ProblemaCopia::Fsck {
                repo: repo
                    .strip_prefix(copia_.carpeta())
                    .unwrap_or(repo)
                    .to_path_buf(),
                detalle: TextoExterno::de(&error),
            })
        })?;
        al_progresar(ProgresoMover::Verificando {
            hechos: indice as u64 + 1,
            total,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cuentas::mover_textos::ProblemaCopia;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;
    use std::process::Command;

    fn modo(ruta: &Path) -> u32 {
        std::fs::symlink_metadata(ruta)
            .expect("metadatos")
            .permissions()
            .mode()
            & 0o7777
    }

    fn poner_modo(ruta: &Path, modo: u32) {
        std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(modo)).expect("chmod");
    }

    /// Árbol de ejemplo: ficheros 0600, directorio 0700, directorio vacío, enlace
    /// relativo, enlace a algo de fuera y enlace colgante.
    fn crear_origen(raiz: &Path, fuera: &Path) -> PathBuf {
        let origen = raiz.join("origen");
        std::fs::create_dir_all(origen.join("gitea/data")).expect("crear");
        std::fs::create_dir_all(origen.join("vacio")).expect("crear");
        std::fs::write(origen.join("gitmereba.toml"), "login = \"x\"\n").expect("escribir");
        std::fs::write(origen.join("gitea/data/gitea.db"), vec![7u8; 5000]).expect("escribir");
        poner_modo(&origen.join("gitmereba.toml"), 0o600);
        poner_modo(&origen.join("gitea/data/gitea.db"), 0o600);
        poner_modo(&origen.join("gitea/data"), 0o700);
        poner_modo(&origen.join("vacio"), 0o750);
        symlink("gitea/data/gitea.db", origen.join("enlace-relativo")).expect("enlace");
        symlink(fuera, origen.join("enlace-a-fuera")).expect("enlace");
        symlink("no-existe", origen.join("enlace-colgante")).expect("enlace");
        poner_modo(&origen, 0o700);
        origen
    }

    fn sin_progreso(_: ProgresoMover) {}

    fn problema(resultado: Result<(), ErrorCuentas>) -> ProblemaCopia {
        match resultado {
            Err(ErrorCuentas::Copia(problema)) => problema,
            otro => panic!("se esperaba ErrorCuentas::Copia, se obtuvo {otro:?}"),
        }
    }

    #[test]
    fn copia_conserva_modos_y_enlaces_sin_seguirlos() {
        let temporal = tempfile::tempdir().expect("temporal");
        let fuera = tempfile::tempdir().expect("fuera");
        std::fs::write(fuera.path().join("secreto"), "no debe copiarse").expect("escribir");
        let origen = crear_origen(temporal.path(), fuera.path());
        let destino = temporal.path().join("copia");

        copiar_arbol(&origen, &destino, &mut sin_progreso).expect("copiar");

        assert_eq!(modo(&destino), 0o700);
        assert_eq!(modo(&destino.join("gitmereba.toml")), 0o600);
        assert_eq!(modo(&destino.join("gitea/data/gitea.db")), 0o600);
        assert_eq!(modo(&destino.join("gitea/data")), 0o700);
        assert_eq!(modo(&destino.join("vacio")), 0o750);
        assert!(destino.join("vacio").is_dir());
        for enlace in ["enlace-relativo", "enlace-a-fuera", "enlace-colgante"] {
            assert_eq!(
                std::fs::read_link(destino.join(enlace)).expect("leer enlace"),
                std::fs::read_link(origen.join(enlace)).expect("leer enlace"),
                "{enlace}"
            );
        }
        // El enlace a un directorio de fuera no se ha seguido: no se ha copiado su contenido.
        assert!(
            std::fs::symlink_metadata(destino.join("enlace-a-fuera"))
                .expect("metadatos")
                .file_type()
                .is_symlink()
        );
        comparar_arboles(&origen, &destino).expect("el árbol coincide");
    }

    #[test]
    fn copia_con_directorio_de_solo_lectura_conserva_su_modo() {
        let temporal = tempfile::tempdir().expect("temporal");
        let origen = temporal.path().join("origen");
        std::fs::create_dir_all(origen.join("ro")).expect("crear");
        std::fs::write(origen.join("ro/f"), "x").expect("escribir");
        poner_modo(&origen.join("ro"), 0o500);
        let destino = temporal.path().join("copia");

        copiar_arbol(&origen, &destino, &mut sin_progreso).expect("copiar");

        assert_eq!(modo(&destino.join("ro")), 0o500);
        poner_modo(&origen.join("ro"), 0o700);
        poner_modo(&destino.join("ro"), 0o700);
    }

    #[test]
    fn rechaza_un_fichero_especial() {
        let temporal = tempfile::tempdir().expect("temporal");
        let origen = temporal.path().join("origen");
        std::fs::create_dir_all(&origen).expect("crear");
        let _socket =
            std::os::unix::net::UnixListener::bind(origen.join("socket")).expect("crear socket");
        let destino = temporal.path().join("copia");

        let resultado = copiar_arbol(&origen, &destino, &mut sin_progreso);

        assert!(matches!(
            problema(resultado),
            ProblemaCopia::TipoEspecial { ruta } if ruta.ends_with("socket")
        ));
    }

    #[test]
    fn informa_del_progreso_hasta_el_total() {
        let temporal = tempfile::tempdir().expect("temporal");
        let fuera = tempfile::tempdir().expect("fuera");
        let origen = crear_origen(temporal.path(), fuera.path());
        let destino = temporal.path().join("copia");
        let mut eventos = Vec::new();

        copiar_arbol(&origen, &destino, &mut |p| eventos.push(p)).expect("copiar");

        let Some(ProgresoMover::Copiando {
            ficheros,
            ficheros_total,
            bytes,
            bytes_total,
        }) = eventos.last().copied()
        else {
            panic!("el último evento debe ser Copiando: {eventos:?}");
        };
        assert_eq!(ficheros, ficheros_total);
        assert_eq!(bytes, bytes_total);
        assert_eq!(ficheros_total, 2);
        assert_eq!(bytes_total, 5000 + "login = \"x\"\n".len() as u64);
    }

    #[test]
    fn comparar_detecta_lo_que_falta_lo_que_sobra_y_lo_distinto() {
        let temporal = tempfile::tempdir().expect("temporal");
        let fuera = tempfile::tempdir().expect("fuera");
        let origen = crear_origen(temporal.path(), fuera.path());
        let destino = temporal.path().join("copia");
        copiar_arbol(&origen, &destino, &mut sin_progreso).expect("copiar");

        // Falta.
        std::fs::remove_file(destino.join("gitmereba.toml")).expect("borrar");
        assert!(matches!(
            problema(comparar_arboles(&origen, &destino)),
            ProblemaCopia::Falta { .. }
        ));
        std::fs::copy(
            origen.join("gitmereba.toml"),
            destino.join("gitmereba.toml"),
        )
        .expect("restaurar");

        // Sobra.
        std::fs::write(destino.join("extra"), "x").expect("escribir");
        assert!(matches!(
            problema(comparar_arboles(&origen, &destino)),
            ProblemaCopia::Sobra { .. }
        ));
        std::fs::remove_file(destino.join("extra")).expect("borrar");

        // Tamaño distinto.
        std::fs::write(destino.join("gitea/data/gitea.db"), vec![7u8; 10]).expect("escribir");
        assert!(matches!(
            problema(comparar_arboles(&origen, &destino)),
            ProblemaCopia::Distinta { .. }
        ));
        std::fs::write(destino.join("gitea/data/gitea.db"), vec![7u8; 5000]).expect("escribir");

        // Modo distinto.
        poner_modo(&destino.join("gitea/data/gitea.db"), 0o644);
        assert!(matches!(
            problema(comparar_arboles(&origen, &destino)),
            ProblemaCopia::Distinta { .. }
        ));
        poner_modo(&destino.join("gitea/data/gitea.db"), 0o600);

        // Destino de enlace distinto.
        std::fs::remove_file(destino.join("enlace-relativo")).expect("borrar");
        symlink("otro", destino.join("enlace-relativo")).expect("enlace");
        assert!(matches!(
            problema(comparar_arboles(&origen, &destino)),
            ProblemaCopia::Distinta { .. }
        ));
    }

    #[test]
    fn los_hashes_detectan_un_cambio_que_conserva_el_tamano() {
        let temporal = tempfile::tempdir().expect("temporal");
        let fuera = tempfile::tempdir().expect("fuera");
        let origen = crear_origen(temporal.path(), fuera.path());
        let destino = temporal.path().join("copia");
        copiar_arbol(&origen, &destino, &mut sin_progreso).expect("copiar");
        comparar_hashes(&origen, &destino).expect("coinciden");

        let mut contenido = vec![7u8; 5000];
        contenido[2500] = 8;
        std::fs::write(destino.join("gitea/data/gitea.db"), contenido).expect("escribir");
        comparar_arboles(&origen, &destino).expect("mismo tamaño y modo");
        assert!(matches!(
            problema(comparar_hashes(&origen, &destino)),
            ProblemaCopia::HashDistinto { ruta } if ruta.ends_with("gitea.db")
        ));
    }

    fn git(args: &[&str], directorio: &Path) {
        let estado = Command::new("git")
            .args(args)
            .current_dir(directorio)
            .status()
            .expect("ejecutar git");
        assert!(estado.success(), "git {args:?}");
    }

    #[tokio::test]
    async fn fsck_acepta_repos_sanos_y_rechaza_uno_corrupto() {
        let temporal = tempfile::tempdir().expect("temporal");
        let cuenta = RutasCuenta::nueva(temporal.path());
        let repo = cuenta.gitea_repositorios().join("ana").join("web.git");
        let snapshot = cuenta.snapshots().join("ana").join("web.git");
        for ruta in [&repo, &snapshot] {
            std::fs::create_dir_all(ruta).expect("crear");
            git(&["init", "--bare", "-q"], ruta);
        }
        let mut eventos = Vec::new();
        fsck_repositorios(&cuenta, &mut |p| eventos.push(p))
            .await
            .expect("repos sanos");
        assert_eq!(
            eventos.last(),
            Some(&ProgresoMover::Verificando {
                hechos: 2,
                total: 2
            })
        );

        // Un blob suelto corrupto.
        let salida = Command::new("sh")
            .arg("-c")
            .arg("echo contenido | git hash-object -w --stdin")
            .current_dir(&repo)
            .output()
            .expect("hash-object");
        let sha = String::from_utf8(salida.stdout)
            .expect("utf8")
            .trim()
            .to_string();
        let objeto = repo.join("objects").join(&sha[..2]).join(&sha[2..]);
        poner_modo(&objeto, 0o644);
        std::fs::write(&objeto, b"basura que no es zlib").expect("corromper");

        let resultado = fsck_repositorios(&cuenta, &mut sin_progreso).await;
        assert!(matches!(
            problema(resultado),
            ProblemaCopia::Fsck { repo, .. } if repo.ends_with("web.git")
        ));
    }
}
