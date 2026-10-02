//! Localización validada de un bare en disco.
//!
//! Copia deliberada de la misma defensa que `verificacion::recogida::ruta_bare_validada`
//! (privada allí, así que no se puede reutilizar desde aquí): construye la ruta solo con
//! [`Nombre`] ya validados y comprueba con `canonicalize` que, tras resolver enlaces
//! simbólicos, sigue quedando dentro de la raíz de `repositories/`.

use std::path::{Path, PathBuf};

use crate::modelo::IdRepo;

fn minusculas(valor: &str) -> String {
    valor.to_lowercase()
}

/// Ruta del bare de `id` bajo `raiz` (la `gitea_repositorios()` de una cuenta), o `None`
/// si no existe o queda fuera de `raiz` tras resolver enlaces simbólicos.
pub(crate) fn ruta_bare_validada(raiz: &Path, id: &IdRepo) -> Option<PathBuf> {
    let candidata = raiz
        .join(minusculas(id.dueno.as_str()))
        .join(format!("{}.git", minusculas(id.nombre.as_str())));
    let real = candidata.canonicalize().ok()?;
    let raiz_real = raiz.canonicalize().ok()?;
    if real.starts_with(&raiz_real) {
        Some(real)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use crate::modelo::Nombre;

    use super::*;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("nombre de prueba válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre de prueba válido"),
        }
    }

    #[test]
    fn encuentra_el_bare_existente_en_minusculas() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let bare = raiz.path().join("jparga").join("repo1.git");
        std::fs::create_dir_all(&bare).expect("crear el bare");

        let encontrada =
            ruta_bare_validada(raiz.path(), &id("JParga", "Repo1")).expect("debe encontrarse");
        assert_eq!(encontrada, bare.canonicalize().expect("canonicalize"));
    }

    #[test]
    fn devuelve_none_si_no_existe() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        assert!(ruta_bare_validada(raiz.path(), &id("jparga", "no-existe")).is_none());
    }

    #[test]
    fn rechaza_un_enlace_simbolico_fuera_de_la_raiz() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        std::fs::create_dir_all(raiz.path().join("jparga")).expect("crear carpeta del dueño");
        let fuera = tempfile::tempdir().expect("directorio fuera de la raíz");
        symlink(fuera.path(), raiz.path().join("jparga").join("repo1.git")).expect("crear enlace");

        assert!(ruta_bare_validada(raiz.path(), &id("jparga", "repo1")).is_none());
    }
}
