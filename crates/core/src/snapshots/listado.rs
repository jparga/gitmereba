//! Listado de capturas: de un repo (`listar`) o de toda la cuenta (`listar_cuenta`).

use time::OffsetDateTime;

use crate::config::RutasCuenta;
use crate::modelo::{IdRepo, Nombre};

use super::error::ErrorSnapshots;
use super::manifiesto::{DIR_MANIFIESTOS, leer_todos};
use super::rutas::{ruta_bare_snapshot_existente, ruta_candidata};

/// Resumen de una captura, sin las refs completas (para eso está `Manifiesto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumenCaptura {
    pub id: IdRepo,
    pub marca: String,
    pub momento: OffsetDateTime,
    pub protegida: bool,
    pub num_refs: usize,
}

/// Lista las capturas de `id`, de más antigua a más reciente. Si nunca se ha
/// capturado, devuelve una lista vacía, no un error.
pub fn listar(rutas: &RutasCuenta, id: &IdRepo) -> Result<Vec<ResumenCaptura>, ErrorSnapshots> {
    let candidata = ruta_candidata(&rutas.snapshots(), id);
    if !candidata.exists() {
        return Ok(Vec::new());
    }
    let destino = ruta_bare_snapshot_existente(&rutas.snapshots(), id)?;
    let manifiestos = leer_todos(&destino.join(DIR_MANIFIESTOS))?;
    Ok(manifiestos
        .into_iter()
        .map(|m| resumen_de(id.clone(), m))
        .collect())
}

fn resumen_de(id: IdRepo, manifiesto: super::manifiesto::Manifiesto) -> ResumenCaptura {
    ResumenCaptura {
        id,
        marca: manifiesto.id,
        momento: manifiesto.momento,
        protegida: manifiesto.protegida,
        num_refs: manifiesto.refs.len(),
    }
}

/// Lista las capturas de todos los repos de la cuenta, recorriendo `snapshots/` sin
/// seguir enlaces simbólicos (`DirEntry::file_type` no los sigue).
pub fn listar_cuenta(rutas: &RutasCuenta) -> Result<Vec<ResumenCaptura>, ErrorSnapshots> {
    let raiz = rutas.snapshots();
    let mut resultado = Vec::new();

    let duenos = match std::fs::read_dir(&raiz) {
        Ok(entradas) => entradas,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(resultado),
        Err(error) => return Err(ErrorSnapshots::Io(error.to_string())),
    };

    for entrada_dueno in duenos {
        let entrada_dueno = entrada_dueno.map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
        if !entrada_dueno
            .file_type()
            .map_err(|error| ErrorSnapshots::Io(error.to_string()))?
            .is_dir()
        {
            continue;
        }
        let Some(nombre_dueno) = entrada_dueno.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(dueno) = Nombre::nuevo(nombre_dueno) else {
            continue;
        };

        let repos = match std::fs::read_dir(entrada_dueno.path()) {
            Ok(entradas) => entradas,
            Err(_) => continue,
        };
        for entrada_repo in repos {
            let entrada_repo =
                entrada_repo.map_err(|error| ErrorSnapshots::Io(error.to_string()))?;
            if !entrada_repo
                .file_type()
                .map_err(|error| ErrorSnapshots::Io(error.to_string()))?
                .is_dir()
            {
                continue;
            }
            let nombre_fichero = entrada_repo.file_name().to_string_lossy().into_owned();
            let Some(nombre_sin_git) = nombre_fichero.strip_suffix(".git") else {
                continue;
            };
            let Ok(nombre) = Nombre::nuevo(nombre_sin_git) else {
                continue;
            };
            let id = IdRepo {
                dueno: dueno.clone(),
                nombre,
            };
            let manifiestos = leer_todos(&entrada_repo.path().join(DIR_MANIFIESTOS))?;
            resultado.extend(manifiestos.into_iter().map(|m| resumen_de(id.clone(), m)));
        }
    }
    Ok(resultado)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(dueno: &str, nombre: &str) -> IdRepo {
        IdRepo {
            dueno: Nombre::nuevo(dueno).expect("dueño válido"),
            nombre: Nombre::nuevo(nombre).expect("nombre válido"),
        }
    }

    #[test]
    fn listar_sin_capturas_da_lista_vacia() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        assert_eq!(
            listar(&rutas, &id("jparga", "repo1")).expect("no falla"),
            Vec::new()
        );
    }

    #[test]
    fn listar_cuenta_sin_snapshots_da_lista_vacia() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        assert_eq!(listar_cuenta(&rutas).expect("no falla"), Vec::new());
    }

    #[test]
    fn listar_cuenta_ignora_un_enlace_simbolico_al_nivel_de_dueno() {
        let raiz = tempfile::tempdir().expect("directorio temporal");
        let rutas = RutasCuenta::nueva(raiz.path().join("cuenta"));
        std::fs::create_dir_all(rutas.snapshots()).expect("crear snapshots/");
        let fuera = tempfile::tempdir().expect("fuera");
        std::os::unix::fs::symlink(fuera.path(), rutas.snapshots().join("enlace"))
            .expect("crear enlace simbólico");

        assert_eq!(listar_cuenta(&rutas).expect("no falla"), Vec::new());
    }
}
