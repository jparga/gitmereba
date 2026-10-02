//! Construcción y validación de rutas de disco: nunca se toca un bare sin comprobar
//! antes, con `canonicalize`, que sigue dentro de la carpeta que le corresponde.

use std::path::{Path, PathBuf};

use crate::config::RutasCuenta;
use crate::modelo::IdRepo;

use super::error::ErrorSnapshots;
use super::fichero::crear_directorio_privado;

fn minusculas(valor: &str) -> String {
    valor.to_lowercase()
}

/// Nombre de fichero del bare de `id`: `<nombre-en-minúsculas>.git`.
fn nombre_bare(id: &IdRepo) -> String {
    format!("{}.git", minusculas(id.nombre.as_str()))
}

/// Ruta (sin comprobar ni crear nada) donde debería estar el bare de `id` bajo `raiz`.
pub(super) fn ruta_candidata(raiz: &Path, id: &IdRepo) -> PathBuf {
    raiz.join(minusculas(id.dueno.as_str()))
        .join(nombre_bare(id))
}

/// Ruta del bare de origen (el mirror de Gitea) de `id`, comprobando que, tras resolver
/// enlaces simbólicos, sigue dentro de `raiz` (`gitea_repositorios()`). Nunca ejecuta
/// `git` sobre una ruta que no haya pasado antes esta comprobación.
pub(super) fn ruta_bare_origen(raiz: &Path, id: &IdRepo) -> Result<PathBuf, ErrorSnapshots> {
    let candidata = ruta_candidata(raiz, id);
    let real = candidata
        .canonicalize()
        .map_err(|_| ErrorSnapshots::OrigenNoExiste)?;
    let raiz_real = raiz
        .canonicalize()
        .map_err(|_| ErrorSnapshots::OrigenNoExiste)?;
    if real.starts_with(&raiz_real) {
        Ok(real)
    } else {
        Err(ErrorSnapshots::OrigenFueraDeRepositorios)
    }
}

/// Igual que [`ruta_bare_origen`] pero para un bare de snapshots ya existente bajo
/// `raiz` (`snapshots()`), con el error propio de «no hay capturas» cuando no existe.
pub(super) fn ruta_bare_snapshot_existente(
    raiz: &Path,
    id: &IdRepo,
) -> Result<PathBuf, ErrorSnapshots> {
    let candidata = ruta_candidata(raiz, id);
    let real = candidata
        .canonicalize()
        .map_err(|_| ErrorSnapshots::SinCapturas)?;
    let raiz_real = raiz
        .canonicalize()
        .map_err(|_| ErrorSnapshots::SinCapturas)?;
    if real.starts_with(&raiz_real) {
        Ok(real)
    } else {
        Err(ErrorSnapshots::DestinoFueraDeSnapshots)
    }
}

/// Asegura que existe el bare propio de snapshots de `id` bajo `raiz`
/// (`snapshots()`): crea los directorios intermedios con permisos 0700 y comprueba,
/// tras resolver enlaces simbólicos, que el resultado sigue dentro de `raiz`. No
/// inicializa el repositorio git en sí (eso lo hace quien llama, con `git init`).
pub(super) fn asegurar_directorio_bare_destino(
    raiz: &Path,
    id: &IdRepo,
) -> Result<PathBuf, ErrorSnapshots> {
    crear_directorio_privado(raiz)?;
    let directorio_dueno = raiz.join(minusculas(id.dueno.as_str()));
    crear_directorio_privado(&directorio_dueno)?;

    let raiz_real = raiz
        .canonicalize()
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    let dueno_real = directorio_dueno
        .canonicalize()
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    if !dueno_real.starts_with(&raiz_real) {
        return Err(ErrorSnapshots::DestinoFueraDeSnapshots);
    }

    let destino = dueno_real.join(nombre_bare(id));
    crear_directorio_privado(&destino)?;
    // El propio directorio del bare debe quedar dentro también: repite la comprobación
    // sobre él, no solo sobre su padre, por si en el futuro `nombre_bare` cambiase.
    let destino_real = destino
        .canonicalize()
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    if destino_real.starts_with(&raiz_real) {
        Ok(destino_real)
    } else {
        Err(ErrorSnapshots::DestinoFueraDeSnapshots)
    }
}

/// Valida el destino de una restauración: no puede existir ya, y su ruta (una vez
/// resuelto el directorio padre) no puede caer dentro de `gitea/` ni de `snapshots/`.
pub(super) fn validar_destino_restauracion(
    rutas: &RutasCuenta,
    destino: &Path,
) -> Result<(), ErrorSnapshots> {
    if std::fs::symlink_metadata(destino).is_ok() {
        return Err(ErrorSnapshots::DestinoExiste);
    }
    let padre = destino.parent().ok_or(ErrorSnapshots::DestinoInvalido)?;
    let nombre = destino.file_name().ok_or(ErrorSnapshots::DestinoInvalido)?;
    let padre_real = padre
        .canonicalize()
        .map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
    let candidato_real = padre_real.join(nombre);

    let dentro_de = |base: PathBuf| -> bool {
        base.canonicalize()
            .map(|base_real| candidato_real.starts_with(&base_real))
            .unwrap_or(false)
    };
    if dentro_de(rutas.gitea()) || dentro_de(rutas.snapshots()) {
        return Err(ErrorSnapshots::DestinoDentroDeGiteaOSnapshots);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::modelo::Nombre;

    use super::*;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("dueño válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre válido"),
        }
    }

    #[test]
    fn ruta_candidata_usa_minusculas_y_sufijo_git() {
        let raiz = PathBuf::from("/raiz");
        assert_eq!(
            ruta_candidata(&raiz, &id("JParga", "Repo1")),
            PathBuf::from("/raiz/jparga/repo1.git")
        );
    }

    #[test]
    fn ruta_bare_origen_falla_si_no_existe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let resultado = ruta_bare_origen(raiz.path(), &id("jparga", "repo1"));
        assert!(matches!(resultado, Err(ErrorSnapshots::OrigenNoExiste)));
    }

    #[test]
    fn ruta_bare_origen_acepta_un_bare_existente() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let bare = raiz.path().join("jparga").join("repo1.git");
        std::fs::create_dir_all(&bare).expect("crear bare");

        let resultado = ruta_bare_origen(raiz.path(), &id("jparga", "repo1")).expect("no falla");
        assert_eq!(resultado, bare.canonicalize().expect("canonicalize"));
    }

    #[test]
    fn ruta_bare_origen_rechaza_un_enlace_simbolico_que_escapa_de_la_raiz() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        std::fs::create_dir_all(raiz.path().join("jparga")).expect("crear carpeta del dueño");
        let fuera = tempfile::tempdir().expect("directorio fuera de la raíz");
        std::os::unix::fs::symlink(fuera.path(), raiz.path().join("jparga").join("repo1.git"))
            .expect("crear enlace simbólico");

        let resultado = ruta_bare_origen(raiz.path(), &id("jparga", "repo1"));
        assert!(matches!(
            resultado,
            Err(ErrorSnapshots::OrigenFueraDeRepositorios)
        ));
    }

    #[test]
    fn asegurar_directorio_bare_destino_crea_con_permisos_0700() {
        use std::os::unix::fs::PermissionsExt;

        let contenedor = tempfile::tempdir().expect("directorio temporal");
        // `raiz` (equivalente a `snapshots()`) no existe todavía: la función debe
        // crearla, junto con la carpeta del dueño y la del bare, las tres con 0700.
        let raiz = contenedor.path().join("snapshots");
        let destino =
            asegurar_directorio_bare_destino(&raiz, &id("jparga", "repo1")).expect("no falla");

        assert!(destino.ends_with("jparga/repo1.git"));
        for ruta in [raiz.clone(), raiz.join("jparga"), destino] {
            let permisos = std::fs::metadata(&ruta).expect("metadata").permissions();
            assert_eq!(permisos.mode() & 0o777, 0o700, "{}", ruta.display());
        }
    }

    #[test]
    fn validar_destino_restauracion_rechaza_uno_que_ya_existe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let destino = raiz.path().join("ya-existe");
        std::fs::create_dir_all(&destino).expect("crear destino");

        assert!(matches!(
            validar_destino_restauracion(&rutas, &destino),
            Err(ErrorSnapshots::DestinoExiste)
        ));
    }

    #[test]
    fn validar_destino_restauracion_rechaza_dentro_de_gitea() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        std::fs::create_dir_all(rutas.gitea()).expect("crear gitea/");
        let destino = rutas.gitea().join("restaurado.git");

        assert!(matches!(
            validar_destino_restauracion(&rutas, &destino),
            Err(ErrorSnapshots::DestinoDentroDeGiteaOSnapshots)
        ));
    }

    #[test]
    fn validar_destino_restauracion_rechaza_dentro_de_snapshots() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        std::fs::create_dir_all(rutas.snapshots()).expect("crear snapshots/");
        let destino = rutas.snapshots().join("restaurado.git");

        assert!(matches!(
            validar_destino_restauracion(&rutas, &destino),
            Err(ErrorSnapshots::DestinoDentroDeGiteaOSnapshots)
        ));
    }

    #[test]
    fn validar_destino_restauracion_acepta_uno_valido() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        let fuera = raiz.path().join("fuera");
        std::fs::create_dir_all(&fuera).expect("crear carpeta fuera");
        let destino = fuera.join("restaurado.git");

        validar_destino_restauracion(&rutas, &destino).expect("no falla");
    }
}
