//! Mover la carpeta de una cuenta a otro sitio.
//!
//! Orden de los pasos (cada uno, salvo el último, se puede deshacer):
//!
//! 1. Validar el destino y la carpeta actual; negarse si hay una sincronización en curso.
//! 2. Parar Gitea (y con él el temporizador, según el [`Lanzador`]).
//! 3. Mover: `rename` si es el mismo sistema de ficheros; si no (`EXDEV`), copia a un
//!    directorio hermano del destino, verificación completa (árbol, SHA-256 de la base de
//!    datos y de la configuración, `git fsck` de cada repo) y `rename` al destino.
//! 4. Repuntar, todo hacia la carpeta nueva: `gitmereba.toml`, `app.ini` (conservando los
//!    secretos), las unidades de systemd, los hooks de git de Gitea y, si hay acceso LAN,
//!    las rutas del certificado. Arrancar Gitea y esperar a que responda.
//! 5. **Punto de confirmación**: escribir el índice `cuentas.toml` (atómico).
//! 6. Borrar la carpeta antigua (solo en la copia: tras un `rename` ya no existe) y
//!    auditar.
//!
//! Un fallo **antes** del punto de confirmación deja la cuenta como estaba: se borra la
//! copia o se deshace el `rename`, se restauran `app.ini`, `gitmereba.toml`, los hooks y
//! las unidades, y se rearranca Gitea en la carpeta antigua; se devuelve el error
//! original. Un fallo **después** (borrar el origen, auditar) no es un error: el
//! movimiento está hecho y el resultado lleva un [`AvisoMover`].

use std::io;
use std::path::{Path, PathBuf};

use crate::config::{self, RutasCuenta};
use crate::idioma::TextoExterno;
use crate::instancia::{
    self, ParametrosLanAppIni, ParametrosUnidad, regenerar_app_ini_conservando_secretos,
};
use crate::modelo::{Cuenta, Nombre};
use crate::secretos::{Llavero, Secreto};

use super::baja::verificar_carpeta_borrable;
use super::comun::{ruta_binario_cacheado, usuario_actual};
use super::contexto::Contexto;
use super::error::ErrorCuentas;
use super::lanzador::{Lanzador, ParametrosLanzamiento};
use super::mover_copia::{
    comparar_arboles, comparar_hashes, copiar_en, crear_raiz_copia, fsck_repositorios,
};
use super::mover_destino::{ruta_parcial, validar_destino};
use super::mover_textos::{AvisoMover, ModoMover, ProgresoMover, ResultadoMover};
use super::temporizador::textos_esperados;

/// Cómo se renombra una carpeta. Es un trait para poder probar el caso de otro sistema de
/// ficheros (`EXDEV`) sin tener dos discos.
pub trait Renombrador {
    fn renombrar(&self, de: &Path, a: &Path) -> io::Result<()>;
}

/// El `rename` del sistema operativo.
pub struct RenombradoReal;

impl Renombrador for RenombradoReal {
    fn renombrar(&self, de: &Path, a: &Path) -> io::Result<()> {
        std::fs::rename(de, a)
    }
}

/// Qué se ha hecho ya, para saber qué deshacer.
struct Estado {
    modo: ModoMover,
    parada: bool,
    /// `rename` del origen al destino hecho.
    origen_renombrado: bool,
    /// Se ha empezado a crear la copia parcial.
    parcial_creado: bool,
    /// La copia ya está en el destino.
    copia_en_destino: bool,
    /// El destino era un directorio vacío (con este modo) y hay que dejarlo así.
    destino_vacio_con_modo: Option<u32>,
    toml_modificado: bool,
    app_ini_tocado: bool,
    unidades_tocadas: bool,
    hooks_tocados: bool,
}

/// Lo que [`previsualizar_mover`] dice que pasaría.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrevisualizacionMover {
    /// Carpeta actual de la cuenta.
    pub origen: PathBuf,
    /// Destino normalizado.
    pub destino: PathBuf,
}

/// Comprueba, sin tocar nada, que `destino` sirve para mover la cuenta `login`, para poder
/// pedir confirmación solo si es válido.
pub fn previsualizar_mover(
    rutas: &crate::config::Rutas,
    login: &Nombre,
    destino: &Path,
) -> Result<PrevisualizacionMover, ErrorCuentas> {
    let indice = config::leer_indice_cuentas(rutas)?;
    let origen = indice
        .cuentas
        .get(login.as_str())
        .ok_or_else(|| ErrorCuentas::CuentaNoExiste(login.clone()))?
        .carpeta
        .clone();
    let destino = validar_destino(rutas, &indice, login, &origen, destino)?;
    verificar_carpeta_borrable(rutas, login, &origen)?;
    Ok(PrevisualizacionMover { origen, destino })
}

/// Mueve la carpeta de la cuenta `login` a `destino`. Ver la documentación del módulo.
///
/// `ejecutable` es la ruta persistente del propio `gitmereba`
/// (`entorno::ruta_ejecutable_persistente`), que va en la unidad del temporizador.
pub async fn mover_carpeta<L: Llavero, Ln: Lanzador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
    destino: &Path,
    ejecutable: &Path,
    al_progresar: &mut dyn FnMut(ProgresoMover),
) -> Result<ResultadoMover, ErrorCuentas> {
    mover_carpeta_con(
        contexto,
        lanzador,
        login,
        destino,
        ejecutable,
        al_progresar,
        &RenombradoReal,
    )
    .await
}

/// Como [`mover_carpeta`], con el renombrado inyectado.
pub async fn mover_carpeta_con<L: Llavero, Ln: Lanzador, R: Renombrador>(
    contexto: &Contexto<'_, L>,
    lanzador: &Ln,
    login: &Nombre,
    destino: &Path,
    ejecutable: &Path,
    al_progresar: &mut dyn FnMut(ProgresoMover),
    renombrador: &R,
) -> Result<ResultadoMover, ErrorCuentas> {
    let rutas = contexto.rutas;
    al_progresar(ProgresoMover::Validando);
    let indice = config::leer_indice_cuentas(rutas)?;
    let entrada = indice
        .cuentas
        .get(login.as_str())
        .ok_or_else(|| ErrorCuentas::CuentaNoExiste(login.clone()))?;
    let origen = entrada.carpeta.clone();
    let cuenta = config::leer_cuenta(&RutasCuenta::nueva(&origen))?;
    let destino = validar_destino(rutas, &indice, login, &origen, destino)?;
    // Lo mismo que exige la baja para borrar: es de verdad la carpeta de esta cuenta y no
    // una ruta protegida. Si no, ni se empieza.
    verificar_carpeta_borrable(rutas, login, &origen)?;

    if lanzador.sincronizacion_en_curso(login).await? {
        return Err(ErrorCuentas::SincronizacionEnCurso(login.clone()));
    }

    let app_ini_original = Secreto::nuevo(
        std::fs::read_to_string(RutasCuenta::nueva(&origen).gitea_app_ini())
            .map_err(|error| ErrorCuentas::Io(error.to_string()))?,
    );
    let directorio_unidades = rutas.directorio_systemd_usuario();
    let tenia_unidad_gitea = directorio_unidades
        .join(instancia::nombre_unidad(login))
        .exists();
    let tenia_unidades_sync = directorio_unidades
        .join(instancia::nombre_servicio_sync(login))
        .exists();
    let contexto_mover = ContextoMover {
        contexto,
        lanzador,
        login,
        origen: &origen,
        destino: &destino,
        cuenta: &cuenta,
        ejecutable,
        app_ini_original: &app_ini_original,
        tenia_unidad_gitea,
        tenia_unidades_sync,
        renombrador,
    };
    let mut estado = Estado {
        modo: ModoMover::Renombrado,
        parada: false,
        origen_renombrado: false,
        parcial_creado: false,
        copia_en_destino: false,
        destino_vacio_con_modo: vacio_con_modo(&destino),
        toml_modificado: false,
        app_ini_tocado: false,
        unidades_tocadas: false,
        hooks_tocados: false,
    };

    if let Err(error) = contexto_mover.ejecutar(&mut estado, al_progresar).await {
        let problemas = contexto_mover.deshacer(&estado).await;
        for problema in &problemas {
            tracing::warn!(%problema, "no se pudo deshacer del todo el movimiento de la carpeta");
        }
        let _ = contexto.almacen.auditar(
            Some(login),
            "cuenta.mover_fallido",
            &format!(
                "origen={} destino={} modo={} deshecho_completo={}",
                origen.display(),
                destino.display(),
                estado.modo.identificador(),
                problemas.is_empty()
            ),
        );
        return Err(error);
    }

    // La cuenta ya vive en la carpeta nueva: a partir de aquí nada es un error.
    let mut avisos = Vec::new();
    if estado.modo == ModoMover::Copia {
        al_progresar(ProgresoMover::BorrandoOrigen);
        let borrado = verificar_carpeta_borrable(rutas, login, &origen).and_then(|()| {
            std::fs::remove_dir_all(&origen).map_err(|error| ErrorCuentas::Io(error.to_string()))
        });
        if let Err(error) = borrado {
            avisos.push(AvisoMover::OrigenSinBorrar {
                ruta: origen.clone(),
                motivo: TextoExterno::de(&error),
            });
        }
    }
    if let Err(error) = contexto.almacen.auditar(
        Some(login),
        "cuenta.mover",
        &format!(
            "origen={} destino={} modo={}",
            origen.display(),
            destino.display(),
            estado.modo.identificador()
        ),
    ) {
        avisos.push(AvisoMover::AuditoriaSinRegistrar {
            motivo: TextoExterno::de(&error),
        });
    }
    al_progresar(ProgresoMover::Terminado);
    Ok(ResultadoMover {
        origen,
        destino,
        modo: estado.modo,
        avisos,
    })
}

/// Modo del destino si ya existe como directorio vacío.
fn vacio_con_modo(destino: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let metadatos = std::fs::symlink_metadata(destino).ok()?;
    metadatos
        .file_type()
        .is_dir()
        .then(|| metadatos.permissions().mode() & 0o7777)
}

struct ContextoMover<'a, L: Llavero, Ln: Lanzador, R: Renombrador> {
    contexto: &'a Contexto<'a, L>,
    lanzador: &'a Ln,
    login: &'a Nombre,
    origen: &'a Path,
    destino: &'a Path,
    cuenta: &'a Cuenta,
    ejecutable: &'a Path,
    app_ini_original: &'a Secreto,
    tenia_unidad_gitea: bool,
    tenia_unidades_sync: bool,
    renombrador: &'a R,
}

impl<L: Llavero, Ln: Lanzador, R: Renombrador> ContextoMover<'_, L, Ln, R> {
    /// Hasta el punto de confirmación incluido.
    async fn ejecutar(
        &self,
        estado: &mut Estado,
        al_progresar: &mut dyn FnMut(ProgresoMover),
    ) -> Result<(), ErrorCuentas> {
        let rutas = self.contexto.rutas;

        al_progresar(ProgresoMover::ParandoGitea);
        estado.parada = true;
        self.lanzador.parar(rutas, self.login).await?;
        // Si el temporizador disparó entre la comprobación y la parada, hay una
        // sincronización con Gitea ya parado: se renuncia.
        if self.lanzador.sincronizacion_en_curso(self.login).await? {
            return Err(ErrorCuentas::SincronizacionEnCurso(self.login.clone()));
        }

        self.mover_datos(estado, al_progresar).await?;
        al_progresar(ProgresoMover::Repuntando);
        let cuenta_nueva = self.repuntar(estado).await?;

        al_progresar(ProgresoMover::ArrancandoGitea);
        arrancar_en(self.lanzador, rutas, self.login, &cuenta_nueva).await?;

        al_progresar(ProgresoMover::Confirmando);
        let mut indice = config::leer_indice_cuentas(rutas)?;
        let entrada = indice
            .cuentas
            .get_mut(self.login.as_str())
            .ok_or_else(|| ErrorCuentas::CuentaNoExiste(self.login.clone()))?;
        entrada.carpeta = self.destino.to_path_buf();
        config::escribir_indice_cuentas(rutas, &indice)?;
        Ok(())
    }

    /// Paso 3: `rename`, o copia verificada si es otro sistema de ficheros.
    async fn mover_datos(
        &self,
        estado: &mut Estado,
        al_progresar: &mut dyn FnMut(ProgresoMover),
    ) -> Result<(), ErrorCuentas> {
        al_progresar(ProgresoMover::Renombrando);
        match self.renombrador.renombrar(self.origen, self.destino) {
            Ok(()) => {
                estado.origen_renombrado = true;
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
            Err(error) => return Err(ErrorCuentas::Io(error.to_string())),
        }

        estado.modo = ModoMover::Copia;
        let parcial = ruta_parcial(self.destino);
        // La marca va tras crear la raíz: si otro la creó entre la validación y aquí, el
        // deshacer no debe borrar lo que no es nuestro.
        crear_raiz_copia(&parcial)?;
        estado.parcial_creado = true;
        copiar_en(self.origen, &parcial, al_progresar)?;
        al_progresar(ProgresoMover::Verificando {
            hechos: 0,
            total: 0,
        });
        comparar_arboles(self.origen, &parcial)?;
        comparar_hashes(self.origen, &parcial)?;
        fsck_repositorios(&RutasCuenta::nueva(&parcial), al_progresar).await?;
        self.renombrador
            .renombrar(&parcial, self.destino)
            .map_err(|error| ErrorCuentas::Io(error.to_string()))?;
        estado.parcial_creado = false;
        estado.copia_en_destino = true;
        Ok(())
    }

    /// Paso 4, hasta antes de arrancar: todo hacia la carpeta nueva. Devuelve la cuenta
    /// tal como queda.
    async fn repuntar(&self, estado: &mut Estado) -> Result<Cuenta, ErrorCuentas> {
        let rutas = self.contexto.rutas;
        let rutas_nueva = RutasCuenta::nueva(self.destino);
        let mut cuenta_nueva = self.cuenta.clone();
        cuenta_nueva.carpeta = self.destino.to_path_buf();

        estado.toml_modificado = true;
        config::escribir_cuenta(&rutas_nueva, &cuenta_nueva)?;

        estado.app_ini_tocado = true;
        let certificado = rutas_nueva.gitea_tls_cert();
        let clave = rutas_nueva.gitea_tls_key();
        let acceso_lan = cuenta_nueva.lan.as_ref().map(|acceso| ParametrosLanAppIni {
            host: &acceso.host,
            ruta_certificado: &certificado,
            ruta_clave: &clave,
        });
        regenerar_app_ini_conservando_secretos(
            &rutas_nueva,
            cuenta_nueva.puerto,
            &usuario_actual(),
            acceso_lan,
        )?;

        estado.unidades_tocadas = true;
        escribir_unidades(
            rutas,
            self.login,
            &cuenta_nueva,
            self.ejecutable,
            self.tenia_unidad_gitea,
            self.tenia_unidades_sync,
        )?;

        // Los hooks de git de cada repo llevan rutas absolutas al binario y a `--config`.
        estado.hooks_tocados = true;
        regenerar_hooks_de(rutas, &rutas_nueva).await?;
        Ok(cuenta_nueva)
    }

    /// Devuelve la cuenta a como estaba. Mejor esfuerzo: sigue aunque un paso falle y
    /// devuelve los problemas encontrados.
    async fn deshacer(&self, estado: &Estado) -> Vec<String> {
        let rutas = self.contexto.rutas;
        let mut problemas = Vec::new();
        let mut anotar = |paso: &str, resultado: Result<(), ErrorCuentas>| {
            if let Err(error) = resultado {
                problemas.push(format!("{paso}: {error}"));
            }
        };

        if estado.parada {
            anotar("parar Gitea", self.lanzador.parar(rutas, self.login).await);
        }

        let rutas_origen = RutasCuenta::nueva(self.origen);
        match estado.modo {
            ModoMover::Renombrado if estado.origen_renombrado => {
                anotar(
                    "deshacer el renombrado",
                    self.renombrador
                        .renombrar(self.destino, self.origen)
                        .map_err(|e| ErrorCuentas::Io(e.to_string())),
                );
                anotar(
                    "recrear el destino vacío",
                    self.recrear_destino_vacio(estado),
                );
                if estado.toml_modificado {
                    anotar(
                        "restaurar gitmereba.toml",
                        config::escribir_cuenta(&rutas_origen, self.cuenta)
                            .map_err(ErrorCuentas::from),
                    );
                }
                if estado.app_ini_tocado {
                    anotar(
                        "restaurar app.ini",
                        instancia::escribir_app_ini(&rutas_origen, self.app_ini_original.exponer())
                            .map_err(ErrorCuentas::from),
                    );
                }
                if estado.hooks_tocados {
                    anotar(
                        "restaurar los hooks de git",
                        regenerar_hooks_de(rutas, &rutas_origen).await,
                    );
                }
            }
            ModoMover::Renombrado => {}
            ModoMover::Copia => {
                if estado.copia_en_destino {
                    anotar(
                        "borrar la copia",
                        std::fs::remove_dir_all(self.destino)
                            .map_err(|e| ErrorCuentas::Io(e.to_string())),
                    );
                    anotar(
                        "recrear el destino vacío",
                        self.recrear_destino_vacio(estado),
                    );
                } else if estado.parcial_creado {
                    let parcial = ruta_parcial(self.destino);
                    anotar(
                        "borrar la copia parcial",
                        borrar_si_existe(&parcial).map_err(|e| ErrorCuentas::Io(e.to_string())),
                    );
                }
            }
        }

        if estado.parada {
            if estado.unidades_tocadas {
                anotar(
                    "restaurar las unidades",
                    escribir_unidades(
                        rutas,
                        self.login,
                        self.cuenta,
                        self.ejecutable,
                        self.tenia_unidad_gitea,
                        self.tenia_unidades_sync,
                    ),
                );
            }
            anotar(
                "rearrancar Gitea en la carpeta antigua",
                arrancar_en(self.lanzador, rutas, self.login, self.cuenta).await,
            );
        }
        problemas
    }

    fn recrear_destino_vacio(&self, estado: &Estado) -> Result<(), ErrorCuentas> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let Some(modo) = estado.destino_vacio_con_modo else {
            return Ok(());
        };
        let io = |e: io::Error| ErrorCuentas::Io(e.to_string());
        std::fs::DirBuilder::new()
            .mode(modo)
            .create(self.destino)
            .map_err(io)?;
        std::fs::set_permissions(self.destino, std::fs::Permissions::from_mode(modo)).map_err(io)
    }
}

fn borrar_si_existe(ruta: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(ruta) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        otro => otro,
    }
}

/// Reescribe, sin tocar systemd, las unidades que ya existían para que apunten a la
/// carpeta de `cuenta`. `daemon-reload` y el arranque los hace el [`Lanzador`].
fn escribir_unidades(
    rutas: &crate::config::Rutas,
    login: &Nombre,
    cuenta: &Cuenta,
    ejecutable: &Path,
    unidad_gitea: bool,
    unidades_sync: bool,
) -> Result<(), ErrorCuentas> {
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    if unidad_gitea {
        let app_ini = rutas_cuenta.gitea_app_ini();
        let trabajo = rutas_cuenta.gitea();
        let texto = instancia::generar_unidad(&ParametrosUnidad {
            login,
            binario_gitea: &ruta_binario_cacheado(rutas),
            app_ini: &app_ini,
            directorio_trabajo: &trabajo,
            carpeta_cuenta: &cuenta.carpeta,
        });
        instancia::escribir_unidad(rutas, login, &texto)?;
    }
    if unidades_sync {
        let (servicio, timer) = textos_esperados(rutas, cuenta, ejecutable);
        instancia::escribir_temporizador(rutas, login, &servicio, &timer)?;
    }
    Ok(())
}

async fn regenerar_hooks_de(
    rutas: &crate::config::Rutas,
    rutas_cuenta: &RutasCuenta,
) -> Result<(), ErrorCuentas> {
    let app_ini = rutas_cuenta.gitea_app_ini();
    let ini = app_ini
        .to_str()
        .ok_or_else(|| ErrorCuentas::Interno("la ruta de app.ini no es UTF-8".to_string()))?;
    instancia::regenerar_hooks(&ruta_binario_cacheado(rutas), ini, &rutas_cuenta.gitea())
        .await
        .map_err(ErrorCuentas::from)
}

/// Arranca Gitea con las rutas de `cuenta` y espera a que responda.
async fn arrancar_en<Ln: Lanzador>(
    lanzador: &Ln,
    rutas: &crate::config::Rutas,
    login: &Nombre,
    cuenta: &Cuenta,
) -> Result<(), ErrorCuentas> {
    let rutas_cuenta = RutasCuenta::nueva(&cuenta.carpeta);
    let certificado_pem = match &cuenta.lan {
        Some(_) => Some(
            std::fs::read(rutas_cuenta.gitea_tls_cert())
                .map_err(|error| ErrorCuentas::Io(error.to_string()))?,
        ),
        None => None,
    };
    let binario = ruta_binario_cacheado(rutas);
    let app_ini = rutas_cuenta.gitea_app_ini();
    let trabajo = rutas_cuenta.gitea();
    let url_local = cuenta.url_gitea();
    lanzador
        .arrancar(
            rutas,
            &ParametrosLanzamiento {
                login,
                binario_gitea: &binario,
                app_ini: &app_ini,
                directorio_trabajo: &trabajo,
                carpeta_cuenta: &cuenta.carpeta,
                puerto: cuenta.puerto,
                url_local: url_local.clone(),
                certificado_pem: certificado_pem.clone(),
            },
        )
        .await?;
    instancia::esperar_salud(&url_local, certificado_pem.as_deref()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::almacen::Almacen;
    use crate::config::Rutas;
    use crate::cuentas::dobles::LanzadorDoble;
    use crate::cuentas::mover_textos::MotivoDestino;
    use crate::modelo::Alcance;
    use crate::secretos::LlaveroEnMemoria;
    use std::path::PathBuf;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn preparar(raiz: &Path) -> (Rutas, PathBuf) {
        let rutas = Rutas::con_raiz(raiz);
        let carpeta = raiz.join("cuenta");
        std::fs::create_dir_all(&carpeta).expect("crear carpeta");
        let cuenta = Cuenta {
            login: nombre("jparga"),
            carpeta: carpeta.clone(),
            puerto: 33055,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan: None,
        };
        config::escribir_cuenta(&RutasCuenta::nueva(&carpeta), &cuenta).expect("escribir toml");
        let ini = RutasCuenta::nueva(&carpeta).gitea_app_ini();
        std::fs::create_dir_all(ini.parent().expect("padre")).expect("crear conf");
        std::fs::write(&ini, "contenido de app.ini\n").expect("escribir app.ini");
        let mut indice = config::leer_indice_cuentas(&rutas).expect("índice");
        indice.cuentas.insert(
            "jparga".to_string(),
            config::EntradaCuenta {
                carpeta: carpeta.clone(),
                puerto: 33055,
            },
        );
        config::escribir_indice_cuentas(&rutas, &indice).expect("escribir índice");
        (rutas, carpeta)
    }

    #[tokio::test]
    async fn se_niega_si_hay_una_sincronizacion_en_curso_sin_tocar_nada() {
        let temporal = tempfile::tempdir().expect("temporal");
        let (rutas, carpeta) = preparar(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();
        *lanzador.sincronizacion_en_curso.lock().unwrap() = true;
        let destino = temporal.path().join("nueva");

        let resultado = mover_carpeta(
            &contexto,
            &lanzador,
            &nombre("jparga"),
            &destino,
            Path::new("/usr/bin/gitmereba"),
            &mut |_| {},
        )
        .await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::SincronizacionEnCurso(_))
        ));
        assert!(
            lanzador.paradas.lock().unwrap().is_empty(),
            "no se paró Gitea"
        );
        assert!(carpeta.exists() && !destino.exists());
        assert!(almacen.auditoria(10, None).expect("auditoría").is_empty());
    }

    #[tokio::test]
    async fn un_destino_invalido_se_rechaza_antes_de_parar_nada() {
        let temporal = tempfile::tempdir().expect("temporal");
        let (rutas, carpeta) = preparar(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let lanzador = LanzadorDoble::default();

        let resultado = mover_carpeta(
            &contexto,
            &lanzador,
            &nombre("jparga"),
            &carpeta.join("dentro"),
            Path::new("/usr/bin/gitmereba"),
            &mut |_| {},
        )
        .await;

        assert!(matches!(
            resultado,
            Err(ErrorCuentas::DestinoNoValido(
                MotivoDestino::DentroDeLaPropia
            ))
        ));
        assert!(lanzador.paradas.lock().unwrap().is_empty());
    }

    #[test]
    fn previsualizar_devuelve_origen_y_destino_normalizado() {
        let temporal = tempfile::tempdir().expect("temporal");
        let (rutas, carpeta) = preparar(temporal.path());
        let destino = temporal.path().join("nueva");

        let previa = previsualizar_mover(&rutas, &nombre("jparga"), &destino).expect("válido");

        assert_eq!(previa.origen, carpeta);
        assert_eq!(previa.destino, destino.canonicalize().unwrap_or(destino));
        assert!(matches!(
            previsualizar_mover(&rutas, &nombre("jparga"), Path::new("relativa")),
            Err(ErrorCuentas::DestinoNoValido(MotivoDestino::NoAbsoluta))
        ));
    }

    #[tokio::test]
    async fn una_cuenta_que_no_existe_es_un_error() {
        let temporal = tempfile::tempdir().expect("temporal");
        let rutas = Rutas::con_raiz(temporal.path());
        let llavero = LlaveroEnMemoria::nuevo();
        let almacen = Almacen::en_memoria().expect("almacén");
        let contexto = Contexto::nuevo(&rutas, &llavero, &almacen);
        let resultado = mover_carpeta(
            &contexto,
            &LanzadorDoble::default(),
            &nombre("nadie"),
            &temporal.path().join("nueva"),
            Path::new("/usr/bin/gitmereba"),
            &mut |_| {},
        )
        .await;
        assert!(matches!(resultado, Err(ErrorCuentas::CuentaNoExiste(_))));
    }
}
