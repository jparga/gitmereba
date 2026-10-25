//! Regeneración de los hooks de git de los repositorios de Gitea.
//!
//! Los scripts que Gitea deja en `hooks/*.d/gitea` de cada repo llevan rutas absolutas al
//! binario de `gitea` y a `--config=<app.ini>`: al mover la carpeta de una cuenta hay que
//! regenerarlos, o cualquier `git push` al Gitea local falla.

use std::path::Path;
use std::time::Duration;

use super::error::ErrorInstancia;
use super::proceso::ejecutar_gitea;

/// Margen generoso: una cuenta con miles de repositorios tarda.
const TIEMPO_LIMITE: Duration = Duration::from_secs(600);

/// `gitea admin regenerate hooks --config <ini>` con `GITEA_WORK_DIR=directorio_trabajo`:
/// reescribe los hooks de todos los repositorios con las rutas actuales. Pensado para
/// ejecutarse con el servicio parado y con el `app.ini` ya repuntado.
pub async fn regenerar_hooks(
    binario: &Path,
    ini: &str,
    directorio_trabajo: &Path,
) -> Result<(), ErrorInstancia> {
    ejecutar_gitea(
        binario,
        &["admin", "regenerate", "hooks", "--config", ini],
        directorio_trabajo,
        TIEMPO_LIMITE,
    )
    .await
    .map(|_| ())
}
