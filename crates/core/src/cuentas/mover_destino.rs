//! Validación de la carpeta de destino de `mover_carpeta`. Se hace entera antes de tocar
//! nada.

use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use crate::config::{IndiceCuentas, Rutas};
use crate::idioma::TextoExterno;
use crate::modelo::Nombre;

use super::baja::es_ruta_protegida;
use super::error::ErrorCuentas;
use super::mover_textos::MotivoDestino;

/// Sufijo del directorio hermano del destino donde se copia antes de renombrar.
pub(super) const SUFIJO_PARCIAL: &str = ".gitmereba-parcial";

/// Caracteres que romperían `app.ini` (`#` y `;` empiezan un comentario) o una unidad de
/// systemd (`%` y `$` se expanden, `"` y `\` son de citado), además de los de control.
const CARACTERES_RESERVADOS: [char; 6] = ['%', '$', '"', '\\', '#', ';'];

/// Valida `destino` para mover la cuenta `login`, cuya carpeta actual es `origen`, y
/// devuelve la ruta normalizada (padre canónico + nombre) con la que hay que trabajar.
///
/// Reglas: ruta absoluta, UTF-8, sin `..` ni caracteres de control o reservados; el padre
/// existe, es un directorio del usuario actual y se resuelve con `canonicalize` (un enlace
/// simbólico en el camino no puede colarla dentro de otra cuenta); no es `/`, el HOME ni
/// un ancestro suyo, ni está dentro de los datos de la app; no se solapa con ninguna
/// carpeta de cuenta (la propia incluida, en ambos sentidos); y, si existe, es un
/// directorio real y vacío.
pub(super) fn validar_destino(
    rutas: &Rutas,
    indice: &IndiceCuentas,
    login: &Nombre,
    origen: &Path,
    destino: &Path,
) -> Result<PathBuf, ErrorCuentas> {
    validar_destino_con(rutas, indice, login, origen, destino, &|ruta| {
        std::fs::metadata(ruta).map(|metadatos| metadatos.dev())
    })
}

/// Como [`validar_destino`], con el número de dispositivo de una ruta inyectable.
fn validar_destino_con(
    rutas: &Rutas,
    indice: &IndiceCuentas,
    login: &Nombre,
    origen: &Path,
    destino: &Path,
    dispositivo: &dyn Fn(&Path) -> std::io::Result<u64>,
) -> Result<PathBuf, ErrorCuentas> {
    fn no_valido(motivo: MotivoDestino) -> ErrorCuentas {
        ErrorCuentas::DestinoNoValido(motivo)
    }
    fn io(error: &std::io::Error) -> ErrorCuentas {
        no_valido(MotivoDestino::Io {
            detalle: TextoExterno::literal(error.to_string()),
        })
    }

    // El origen debe ser un directorio real: un `rename` de un enlace movería el enlace.
    match std::fs::symlink_metadata(origen) {
        Ok(metadatos) if metadatos.file_type().is_dir() => {}
        _ => {
            return Err(no_valido(MotivoDestino::OrigenNoValido {
                ruta: origen.to_path_buf(),
            }));
        }
    }

    let texto = destino
        .to_str()
        .ok_or_else(|| no_valido(MotivoDestino::NoUtf8))?;
    if !destino.is_absolute() {
        return Err(no_valido(MotivoDestino::NoAbsoluta));
    }
    if destino
        .components()
        .any(|componente| matches!(componente, Component::ParentDir))
    {
        return Err(no_valido(MotivoDestino::ConPuntoPunto));
    }
    // Gitea recorta los espacios de los valores de `app.ini`: un componente así acabaría
    // en otra ruta distinta de la que se creó.
    if texto
        .split('/')
        .any(|componente| componente.trim() != componente)
    {
        return Err(no_valido(MotivoDestino::ComponenteConEspacios));
    }
    if let Some(caracter) = texto
        .chars()
        .find(|c| c.is_control() || CARACTERES_RESERVADOS.contains(c))
    {
        return Err(no_valido(MotivoDestino::CaracterNoPermitido { caracter }));
    }

    // `/` no tiene nombre ni padre.
    let (Some(padre), Some(nombre)) = (destino.parent(), destino.file_name()) else {
        return Err(no_valido(MotivoDestino::Protegida));
    };
    let padre = padre
        .canonicalize()
        .map_err(|_| no_valido(MotivoDestino::SinPadre))?;
    let metadatos_padre =
        std::fs::metadata(&padre).map_err(|_| no_valido(MotivoDestino::SinPadre))?;
    if !metadatos_padre.is_dir() {
        return Err(no_valido(MotivoDestino::SinPadre));
    }
    let destino = padre.join(nombre);

    // Antes que la comprobación del dueño: `/home` o `/` son de root, pero el motivo útil
    // es que están protegidos.
    if es_ruta_protegida(rutas, &destino) {
        return Err(no_valido(MotivoDestino::Protegida));
    }
    if metadatos_padre.uid() != uid_actual().map_err(|e| io(&e))? {
        return Err(no_valido(MotivoDestino::PadreAjeno));
    }
    let datos = rutas.directorio_datos();
    let datos_canonicos = datos.canonicalize().unwrap_or_else(|_| datos.to_path_buf());
    if destino.starts_with(datos) || destino.starts_with(&datos_canonicos) {
        return Err(no_valido(MotivoDestino::DentroDeDatosDeLaApp));
    }

    comprobar_solapes(indice, login, &destino)?;

    let parcial = ruta_parcial(&destino);
    if std::fs::symlink_metadata(&parcial).is_ok() {
        return Err(no_valido(MotivoDestino::ParcialExistente { ruta: parcial }));
    }

    match std::fs::symlink_metadata(&destino) {
        Ok(metadatos) => {
            let tipo = metadatos.file_type();
            if tipo.is_symlink() {
                return Err(no_valido(MotivoDestino::EsEnlaceSimbolico));
            }
            if !tipo.is_dir() {
                return Err(no_valido(MotivoDestino::NoEsDirectorio));
            }
            // Un punto de montaje no se puede reemplazar con `rename`: fallaría al final,
            // tras copiarlo todo.
            if dispositivo(&destino).map_err(|e| io(&e))?
                != dispositivo(&padre).map_err(|e| io(&e))?
            {
                return Err(no_valido(MotivoDestino::EsPuntoDeMontaje));
            }
            let vacio = std::fs::read_dir(&destino)
                .map_err(|e| io(&e))?
                .next()
                .is_none();
            if !vacio {
                return Err(no_valido(MotivoDestino::NoVacio));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(&error)),
    }
    Ok(destino)
}

/// Carpeta hermana del destino donde se copia antes de renombrar.
pub(super) fn ruta_parcial(destino: &Path) -> PathBuf {
    let mut nombre = destino.file_name().map(OsString::from).unwrap_or_default();
    nombre.push(SUFIJO_PARCIAL);
    destino.with_file_name(nombre)
}

fn comprobar_solapes(
    indice: &IndiceCuentas,
    login: &Nombre,
    destino: &Path,
) -> Result<(), ErrorCuentas> {
    for (otro_login, entrada) in &indice.cuentas {
        let carpeta = entrada
            .carpeta
            .canonicalize()
            .unwrap_or_else(|_| entrada.carpeta.clone());
        let es_propia = otro_login == login.as_str();
        let motivo = if destino.starts_with(&carpeta) {
            if es_propia {
                MotivoDestino::DentroDeLaPropia
            } else {
                MotivoDestino::DentroDeOtraCuenta {
                    login: nombre_de(otro_login)?,
                    carpeta,
                }
            }
        } else if carpeta.starts_with(destino) {
            if es_propia {
                MotivoDestino::ContieneLaPropia
            } else {
                MotivoDestino::ContieneOtraCuenta {
                    login: nombre_de(otro_login)?,
                    carpeta,
                }
            }
        } else {
            continue;
        };
        return Err(ErrorCuentas::DestinoNoValido(motivo));
    }
    Ok(())
}

fn nombre_de(login: &str) -> Result<Nombre, ErrorCuentas> {
    Nombre::nuevo(login)
        .map_err(|error| ErrorCuentas::Interno(format!("login inválido en el índice: {error}")))
}

/// Uid del proceso: el dueño de `/proc/self`.
fn uid_actual() -> Result<u32, std::io::Error> {
    std::fs::metadata("/proc/self").map(|metadatos| metadatos.uid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EntradaCuenta;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    struct Escenario {
        _temporal: tempfile::TempDir,
        raiz: PathBuf,
        rutas: Rutas,
        indice: IndiceCuentas,
        propia: PathBuf,
        otra: PathBuf,
    }

    fn escenario() -> Escenario {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path().canonicalize().expect("canonicalizar");
        let rutas = Rutas::con_raiz(&raiz);
        let propia = raiz.join("propia");
        let otra = raiz.join("otra");
        std::fs::create_dir_all(propia.join("gitea")).expect("crear propia");
        std::fs::create_dir_all(otra.join("gitea")).expect("crear otra");
        let mut indice = IndiceCuentas::default();
        for (login, carpeta, puerto) in [("jparga", &propia, 3001), ("otra", &otra, 3002)] {
            indice.cuentas.insert(
                login.to_string(),
                EntradaCuenta {
                    carpeta: carpeta.clone(),
                    puerto,
                },
            );
        }
        Escenario {
            _temporal: temporal,
            raiz,
            rutas,
            indice,
            propia,
            otra,
        }
    }

    fn validar(e: &Escenario, destino: &Path) -> Result<PathBuf, ErrorCuentas> {
        validar_destino(&e.rutas, &e.indice, &nombre("jparga"), &e.propia, destino)
    }

    fn motivo(resultado: Result<PathBuf, ErrorCuentas>) -> MotivoDestino {
        match resultado {
            Err(ErrorCuentas::DestinoNoValido(motivo)) => motivo,
            otro => panic!("se esperaba DestinoNoValido, se obtuvo {otro:?}"),
        }
    }

    #[test]
    fn acepta_una_ruta_que_no_existe() {
        let e = escenario();
        let destino = e.raiz.join("nueva");
        assert_eq!(validar(&e, &destino).expect("válida"), destino);
    }

    #[test]
    fn acepta_un_directorio_vacio() {
        let e = escenario();
        let destino = e.raiz.join("vacia");
        std::fs::create_dir(&destino).expect("crear");
        assert_eq!(validar(&e, &destino).expect("válida"), destino);
    }

    #[test]
    fn rechaza_una_ruta_relativa() {
        let e = escenario();
        assert_eq!(
            motivo(validar(&e, Path::new("relativa/carpeta"))),
            MotivoDestino::NoAbsoluta
        );
    }

    #[test]
    fn rechaza_punto_punto() {
        let e = escenario();
        let destino = e.raiz.join("a").join("..").join("b");
        assert_eq!(motivo(validar(&e, &destino)), MotivoDestino::ConPuntoPunto);
    }

    #[test]
    fn rechaza_un_salto_de_linea() {
        let e = escenario();
        let destino = e.raiz.join("con\nsalto");
        assert_eq!(
            motivo(validar(&e, &destino)),
            MotivoDestino::CaracterNoPermitido { caracter: '\n' }
        );
    }

    #[test]
    fn rechaza_caracteres_que_rompen_las_unidades_o_el_ini() {
        let e = escenario();
        for caracter in ['%', '$', '"', '\\', '#', ';', '\t', '\r'] {
            let destino = e.raiz.join(format!("x{caracter}y"));
            assert_eq!(
                motivo(validar(&e, &destino)),
                MotivoDestino::CaracterNoPermitido { caracter }
            );
        }
    }

    #[test]
    fn rechaza_un_componente_que_empieza_o_acaba_en_espacio_en_blanco() {
        let e = escenario();
        for nombre in ["cuenta ", " cuenta", "\u{a0}cuenta", "a /b"] {
            let destino = e.raiz.join(nombre);
            assert_eq!(
                motivo(validar(&e, &destino)),
                MotivoDestino::ComponenteConEspacios,
                "{nombre:?}"
            );
        }
        // Un espacio en medio es válido.
        assert!(validar(&e, &e.raiz.join("mi cuenta")).is_ok());
    }

    #[test]
    fn rechaza_un_destino_que_es_un_punto_de_montaje() {
        let e = escenario();
        let destino = e.raiz.join("montaje");
        std::fs::create_dir(&destino).expect("crear");
        // Se simula que el destino está en otro dispositivo que su padre.
        let dev = |ruta: &Path| Ok(if ruta == destino { 2 } else { 1 });
        let resultado = validar_destino_con(
            &e.rutas,
            &e.indice,
            &nombre("jparga"),
            &e.propia,
            &destino,
            &dev,
        );
        assert_eq!(motivo(resultado), MotivoDestino::EsPuntoDeMontaje);
        // En el mismo dispositivo que el padre, vale.
        let mismo = |_: &Path| Ok(1);
        assert!(
            validar_destino_con(
                &e.rutas,
                &e.indice,
                &nombre("jparga"),
                &e.propia,
                &destino,
                &mismo
            )
            .is_ok()
        );
    }

    #[test]
    fn rechaza_un_destino_existente_no_vacio() {
        let e = escenario();
        let destino = e.raiz.join("llena");
        std::fs::create_dir(&destino).expect("crear");
        std::fs::write(destino.join("algo"), "x").expect("escribir");
        assert_eq!(motivo(validar(&e, &destino)), MotivoDestino::NoVacio);
    }

    #[test]
    fn rechaza_un_fichero_como_destino() {
        let e = escenario();
        let destino = e.raiz.join("fichero");
        std::fs::write(&destino, "x").expect("escribir");
        assert_eq!(motivo(validar(&e, &destino)), MotivoDestino::NoEsDirectorio);
    }

    #[test]
    fn rechaza_un_enlace_simbolico_aunque_apunte_a_un_directorio_vacio() {
        let e = escenario();
        let vacia = e.raiz.join("vacia");
        std::fs::create_dir(&vacia).expect("crear");
        let enlace = e.raiz.join("enlace");
        std::os::unix::fs::symlink(&vacia, &enlace).expect("enlace");
        assert_eq!(
            motivo(validar(&e, &enlace)),
            MotivoDestino::EsEnlaceSimbolico
        );
    }

    #[test]
    fn rechaza_un_destino_dentro_de_otra_cuenta() {
        let e = escenario();
        let destino = e.otra.join("gitea").join("nueva");
        assert!(matches!(
            motivo(validar(&e, &destino)),
            MotivoDestino::DentroDeOtraCuenta { .. }
        ));
    }

    #[test]
    fn rechaza_un_destino_que_contiene_otra_cuenta() {
        let e = escenario();
        // El destino vacío es el padre común de la otra cuenta solo si no hay más: se
        // construye una otra cuenta dentro de un directorio nuevo.
        let contenedor = e.raiz.join("contenedor");
        let dentro = contenedor.join("cuenta-b");
        std::fs::create_dir_all(&dentro).expect("crear");
        let mut e = e;
        e.indice.cuentas.insert(
            "b".to_string(),
            EntradaCuenta {
                carpeta: dentro,
                puerto: 3003,
            },
        );
        assert!(matches!(
            motivo(validar(&e, &contenedor)),
            MotivoDestino::ContieneOtraCuenta { .. }
        ));
    }

    #[test]
    fn rechaza_un_destino_dentro_de_la_propia_cuenta() {
        let e = escenario();
        assert_eq!(
            motivo(validar(&e, &e.propia.join("sub"))),
            MotivoDestino::DentroDeLaPropia
        );
        assert_eq!(
            motivo(validar(&e, &e.propia)),
            MotivoDestino::DentroDeLaPropia
        );
    }

    #[test]
    fn rechaza_un_destino_que_contiene_la_propia_cuenta() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path().canonicalize().expect("canonicalizar");
        let rutas = Rutas::con_raiz(raiz.join("home"));
        let destino = raiz.join("contenedor");
        let propia = destino.join("cuenta");
        std::fs::create_dir_all(&propia).expect("crear");
        let mut indice = IndiceCuentas::default();
        indice.cuentas.insert(
            "jparga".to_string(),
            EntradaCuenta {
                carpeta: propia.clone(),
                puerto: 3001,
            },
        );
        let resultado = validar_destino(&rutas, &indice, &nombre("jparga"), &propia, &destino);
        // Está lleno (contiene la cuenta), pero el motivo útil es el de contención.
        assert_eq!(motivo(resultado), MotivoDestino::ContieneLaPropia);
    }

    #[test]
    fn rechaza_el_home_la_raiz_y_un_ancestro_del_home() {
        let e = escenario();
        assert_eq!(motivo(validar(&e, &e.raiz)), MotivoDestino::Protegida);
        assert_eq!(
            motivo(validar(&e, Path::new("/"))),
            MotivoDestino::Protegida
        );
        let ancestro = e.raiz.parent().expect("padre").to_path_buf();
        assert_eq!(motivo(validar(&e, &ancestro)), MotivoDestino::Protegida);
    }

    #[test]
    fn rechaza_el_directorio_de_datos_de_la_app() {
        let e = escenario();
        let destino = e.rutas.directorio_datos().join("cuenta");
        std::fs::create_dir_all(e.rutas.directorio_datos()).expect("crear datos");
        assert_eq!(
            motivo(validar(&e, &destino)),
            MotivoDestino::DentroDeDatosDeLaApp
        );
    }

    #[test]
    fn un_enlace_simbolico_en_el_padre_que_apunta_dentro_de_otra_cuenta_se_detecta() {
        let e = escenario();
        // `atajo` apunta a la carpeta de la otra cuenta; el destino cuelga de `atajo`.
        let atajo = e.raiz.join("atajo");
        std::os::unix::fs::symlink(&e.otra, &atajo).expect("enlace");
        let destino = atajo.join("nueva");
        assert!(matches!(
            motivo(validar(&e, &destino)),
            MotivoDestino::DentroDeOtraCuenta { .. }
        ));
    }

    #[test]
    fn rechaza_un_destino_que_es_un_enlace_simbolico_a_otra_cuenta() {
        let e = escenario();
        let enlace = e.raiz.join("enlace-a-otra");
        std::os::unix::fs::symlink(&e.otra, &enlace).expect("enlace");
        assert_eq!(
            motivo(validar(&e, &enlace)),
            MotivoDestino::EsEnlaceSimbolico
        );
    }

    #[test]
    fn rechaza_si_el_padre_no_existe() {
        let e = escenario();
        let destino = e.raiz.join("no-existe").join("nueva");
        assert_eq!(motivo(validar(&e, &destino)), MotivoDestino::SinPadre);
    }

    #[test]
    fn rechaza_si_queda_una_copia_parcial() {
        let e = escenario();
        let destino = e.raiz.join("nueva");
        std::fs::create_dir(e.raiz.join(format!("nueva{SUFIJO_PARCIAL}"))).expect("crear");
        assert!(matches!(
            motivo(validar(&e, &destino)),
            MotivoDestino::ParcialExistente { .. }
        ));
    }

    #[test]
    fn rechaza_si_el_origen_es_un_enlace_simbolico() {
        let e = escenario();
        let real = e.raiz.join("real");
        std::fs::create_dir(&real).expect("crear");
        let enlace = e.raiz.join("enlace-origen");
        std::os::unix::fs::symlink(&real, &enlace).expect("enlace");
        let resultado = validar_destino(
            &e.rutas,
            &e.indice,
            &nombre("jparga"),
            &enlace,
            &e.raiz.join("nueva"),
        );
        assert!(matches!(
            motivo(resultado),
            MotivoDestino::OrigenNoValido { .. }
        ));
    }

    #[test]
    fn rename_sobre_un_directorio_vacio_funciona_en_linux() {
        let e = escenario();
        let vacia = e.raiz.join("vacia");
        std::fs::create_dir(&vacia).expect("crear");
        let origen = e.raiz.join("origen");
        std::fs::create_dir(&origen).expect("crear");
        std::fs::write(origen.join("f"), "x").expect("escribir");
        std::fs::rename(&origen, &vacia).expect("rename sobre un directorio vacío");
        assert!(vacia.join("f").exists());
        assert!(!origen.exists());
    }
}
