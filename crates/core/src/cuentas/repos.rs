//! Incluir o excluir un repo del alcance de una cuenta ya dada de alta (pantalla
//! Repositorios: acción «excluir»/«incluir»).

use crate::almacen::Almacen;
use crate::config::{self, RutasCuenta};
use crate::modelo::{Cuenta, IdRepo};

use super::error::ErrorCuentas;

/// Añade o quita `id` de `Alcance::excluidos` de `cuenta` y lo deja en `gitmereba.toml`.
/// No sincroniza por sí solo: el efecto (pausar o reanudar el mirror) se aplica en la
/// siguiente pasada de `sync::ejecutar`.
///
/// `id` debe pertenecer a `cuenta`: su dueño debe ser el login de la cuenta o una de sus
/// organizaciones en alcance. Si no, es un error (defiende contra un id ajeno colado
/// desde la interfaz).
pub fn excluir_repo(
    almacen: &Almacen,
    rutas_cuenta: &RutasCuenta,
    cuenta: &Cuenta,
    id: &IdRepo,
    excluido: bool,
) -> Result<(), ErrorCuentas> {
    let pertenece = id.dueno == cuenta.login
        || cuenta
            .alcance
            .organizaciones
            .iter()
            .any(|org| org.as_str().eq_ignore_ascii_case(id.dueno.as_str()));
    if !pertenece {
        return Err(ErrorCuentas::RepoNoExiste(id.clone()));
    }

    let mut nueva = cuenta.clone();
    if excluido {
        if !nueva.alcance.excluidos.contains(id) {
            nueva.alcance.excluidos.push(id.clone());
        }
    } else {
        nueva
            .alcance
            .excluidos
            .retain(|excluido_id| excluido_id != id);
    }
    config::escribir_cuenta(rutas_cuenta, &nueva)?;

    let accion = if excluido {
        "repo.excluir"
    } else {
        "repo.incluir"
    };
    almacen.auditar(Some(&cuenta.login), accion, &format!("id={id}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Alcance, Nombre};

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).expect("nombre de prueba válido")
    }

    fn id(dueno: &str, nombre_repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(nombre_repo),
        }
    }

    fn preparar(raiz: &std::path::Path, organizaciones: &[&str]) -> (Almacen, RutasCuenta, Cuenta) {
        let carpeta = raiz.join("cuenta");
        std::fs::create_dir_all(&carpeta).expect("crear carpeta");
        let rutas_cuenta = RutasCuenta::nueva(&carpeta);
        let cuenta = Cuenta {
            login: nombre("jparga"),
            carpeta,
            puerto: 3900,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: organizaciones.iter().map(|o| nombre(o)).collect(),
                excluidos: vec![],
            },
            lan: None,
        };
        config::escribir_cuenta(&rutas_cuenta, &cuenta).expect("escribir gitmereba.toml");
        let almacen = Almacen::en_memoria().expect("almacén en memoria");
        (almacen, rutas_cuenta, cuenta)
    }

    #[test]
    fn excluir_anade_el_repo_y_audita() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path(), &[]);
        let repo = id("jparga", "roto-ci");

        excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, true).expect("excluir no falla");

        let leida = config::leer_cuenta(&rutas_cuenta).expect("releer cuenta");
        assert!(leida.alcance.excluidos.contains(&repo));
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "repo.excluir"));
    }

    #[test]
    fn excluir_dos_veces_es_idempotente() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path(), &[]);
        let repo = id("jparga", "roto-ci");

        excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, true).expect("primera exclusión");
        excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, true).expect("segunda exclusión");

        let leida = config::leer_cuenta(&rutas_cuenta).expect("releer cuenta");
        assert_eq!(
            leida
                .alcance
                .excluidos
                .iter()
                .filter(|excluido| **excluido == repo)
                .count(),
            1
        );
    }

    #[test]
    fn incluir_quita_el_repo_de_excluidos() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, mut cuenta) = preparar(temporal.path(), &[]);
        let repo = id("jparga", "roto-ci");
        cuenta.alcance.excluidos.push(repo.clone());
        config::escribir_cuenta(&rutas_cuenta, &cuenta).expect("preparar excluido");

        excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, false).expect("incluir no falla");

        let leida = config::leer_cuenta(&rutas_cuenta).expect("releer cuenta");
        assert!(!leida.alcance.excluidos.contains(&repo));
        let auditoria = almacen.auditoria(10, None).expect("leer auditoría");
        assert!(auditoria.iter().any(|e| e.accion == "repo.incluir"));
    }

    #[test]
    fn un_repo_de_una_organizacion_en_alcance_es_valido() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path(), &["mereba-oss"]);
        let repo = id("mereba-oss", "gitmereba");

        excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, true).expect("excluir no falla");
    }

    #[test]
    fn un_repo_ajeno_a_la_cuenta_es_un_error() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let (almacen, rutas_cuenta, cuenta) = preparar(temporal.path(), &[]);
        let repo = id("otra-persona", "su-repo");

        let resultado = excluir_repo(&almacen, &rutas_cuenta, &cuenta, &repo, true);

        assert!(matches!(resultado, Err(ErrorCuentas::RepoNoExiste(_))));
    }
}
