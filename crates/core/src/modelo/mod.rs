//! Tipos de dominio compartidos por el resto de módulos.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Error al validar un nombre procedente de GitHub, Gitea o el usuario.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErrorNombre {
    #[error("el nombre está vacío")]
    Vacio,
    #[error("el nombre supera los {0} caracteres")]
    DemasiadoLargo(usize),
    #[error("el nombre contiene caracteres no permitidos")]
    CaracteresNoPermitidos,
    #[error("el nombre no puede empezar por guion, contener «..» ni ser «.» o «.git»")]
    FormaNoPermitida,
}

const LONGITUD_MAXIMA: usize = 100;

/// Nombre de cuenta, organización o repositorio, ya validado.
///
/// Solo admite `[A-Za-z0-9._-]`, sin `..`, sin empezar por `-` (no puede confundirse con
/// una opción de `git`) y sin ser `.` ni `.git`, de modo que es seguro usarlo en rutas,
/// URLs y argumentos. Sí puede empezar por punto: GitHub admite repos como `.github`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Nombre(String);

impl Nombre {
    pub fn nuevo(valor: impl Into<String>) -> Result<Self, ErrorNombre> {
        let valor = valor.into();
        if valor.is_empty() {
            return Err(ErrorNombre::Vacio);
        }
        if valor.len() > LONGITUD_MAXIMA {
            return Err(ErrorNombre::DemasiadoLargo(LONGITUD_MAXIMA));
        }
        if !valor
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(ErrorNombre::CaracteresNoPermitidos);
        }
        if valor.starts_with('-')
            || valor.contains("..")
            || valor == "."
            || valor.eq_ignore_ascii_case(".git")
        {
            return Err(ErrorNombre::FormaNoPermitida);
        }
        Ok(Self(valor))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Nombre {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Nombre {
    type Error = ErrorNombre;

    fn try_from(valor: String) -> Result<Self, Self::Error> {
        Self::nuevo(valor)
    }
}

impl From<Nombre> for String {
    fn from(nombre: Nombre) -> Self {
        nombre.0
    }
}

/// Identifica un repositorio como `dueño/nombre`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdRepo {
    pub dueno: Nombre,
    pub nombre: Nombre,
}

impl fmt::Display for IdRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.dueno, self.nombre)
    }
}

/// Repositorio tal como lo describe GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoOrigen {
    pub id: IdRepo,
    /// URL https de clonado, sin credenciales.
    pub url_clon: String,
    pub privado: bool,
    pub es_fork: bool,
    pub archivado: bool,
    pub rama_por_defecto: Option<String>,
    pub descripcion: Option<String>,
    pub tamano_kb: u64,
}

/// Repositorio tal como existe en el Gitea local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoLocal {
    pub id: IdRepo,
    pub es_mirror: bool,
    pub vacio: bool,
    pub privado: bool,
    pub tamano_kb: u64,
    /// Última sincronización del mirror, si Gitea la informa.
    #[serde(with = "time::serde::rfc3339::option")]
    pub ultima_sync: Option<OffsetDateTime>,
}

/// Estado de un repositorio clonado, de más a menos urgente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EstadoRepo {
    /// El mirror falla o no coincide con GitHub.
    Fallo,
    /// El mirror lleva sin sincronizar más que el umbral.
    Obsoleto,
    /// Ya no existe en GitHub; se conserva en local.
    Huerfano,
    /// Convertido en repo con escritura durante una contingencia.
    Contingencia,
    /// Excluido por el usuario; no se clona.
    Excluido,
    Ok,
}

/// Qué se clona de una cuenta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alcance {
    pub incluir_forks: bool,
    pub organizaciones: Vec<Nombre>,
    pub excluidos: Vec<IdRepo>,
}

/// Error al validar un nombre de host `.internal` (acceso LAN).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErrorHostInterno {
    #[error("el nombre de host está vacío")]
    Vacio,
    #[error("el nombre de host supera los 253 caracteres")]
    DemasiadoLargo,
    #[error("el nombre de host debe terminar en «.internal»")]
    NoTerminaEnInternal,
    #[error("la etiqueta «{0}» del nombre de host no es válida")]
    EtiquetaInvalida(String),
}

const LONGITUD_MAXIMA_HOST: usize = 253;
const LONGITUD_MAXIMA_ETIQUETA: usize = 63;
const SUFIJO_INTERNAL: &str = ".internal";

/// Nombre de host `.internal`, ya validado: etiquetas DNS en minúsculas
/// (`[a-z0-9-]`, 1-63 caracteres, sin empezar ni acabar en `-`), longitud total hasta
/// 253 caracteres y que termina siempre en `.internal` (TLD reservado por ICANN para uso
/// privado: nunca resuelve en Internet).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NombreHostInterno(String);

impl NombreHostInterno {
    pub fn nuevo(valor: impl Into<String>) -> Result<Self, ErrorHostInterno> {
        let valor = valor.into();
        if valor.is_empty() {
            return Err(ErrorHostInterno::Vacio);
        }
        if valor.chars().count() > LONGITUD_MAXIMA_HOST {
            return Err(ErrorHostInterno::DemasiadoLargo);
        }
        let prefijo = valor
            .strip_suffix(SUFIJO_INTERNAL)
            .filter(|prefijo| !prefijo.is_empty())
            .ok_or(ErrorHostInterno::NoTerminaEnInternal)?;
        for etiqueta in prefijo.split('.') {
            validar_etiqueta_host(etiqueta)?;
        }
        Ok(Self(valor))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NombreHostInterno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for NombreHostInterno {
    type Error = ErrorHostInterno;

    fn try_from(valor: String) -> Result<Self, Self::Error> {
        Self::nuevo(valor)
    }
}

impl From<NombreHostInterno> for String {
    fn from(host: NombreHostInterno) -> Self {
        host.0
    }
}

fn validar_etiqueta_host(etiqueta: &str) -> Result<(), ErrorHostInterno> {
    let valida = !etiqueta.is_empty()
        && etiqueta.len() <= LONGITUD_MAXIMA_ETIQUETA
        && etiqueta
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !etiqueta.starts_with('-')
        && !etiqueta.ends_with('-');
    if valida {
        Ok(())
    } else {
        Err(ErrorHostInterno::EtiquetaInvalida(etiqueta.to_string()))
    }
}

/// Construye el host `.internal` por defecto de una cuenta: `<login saneado en
/// minúsculas>.gitmereba.internal`. Saneado por construcción (sin
/// `unwrap`/`expect`): cualquier carácter que no sea `[a-z0-9]` se convierte en `-`, se
/// recortan los guiones de los extremos y, si no queda nada, se usa «cuenta».
pub fn host_lan_por_defecto(login: &Nombre) -> NombreHostInterno {
    let normalizado: String = login
        .as_str()
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let recortado = normalizado.trim_matches('-');
    let etiqueta: String = if recortado.is_empty() {
        "cuenta".to_string()
    } else {
        recortado.chars().take(LONGITUD_MAXIMA_ETIQUETA).collect()
    };
    let etiqueta = etiqueta.trim_end_matches('-');
    let etiqueta = if etiqueta.is_empty() {
        "cuenta"
    } else {
        etiqueta
    };
    NombreHostInterno(format!("{etiqueta}.gitmereba.internal"))
}

/// Acceso a la cuenta desde la LAN por HTTPS con un nombre `.internal` (apagado
/// por defecto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccesoLan {
    pub host: NombreHostInterno,
}

/// Cuenta de GitHub clonada. No contiene secretos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cuenta {
    pub login: Nombre,
    pub carpeta: PathBuf,
    pub puerto: u16,
    pub intervalo_minutos: u32,
    pub alcance: Alcance,
    /// Acceso desde la LAN por HTTPS. `None` (el valor por defecto de una
    /// cuenta nueva): Gitea solo escucha en `127.0.0.1`. Los `gitmereba.toml` de
    /// cuentas anteriores al acceso LAN, que no tienen esta clave, se siguen leyendo igual
    /// gracias a `#[serde(default)]`.
    #[serde(default)]
    pub lan: Option<AccesoLan>,
}

impl Cuenta {
    /// URL base del Gitea local de la cuenta, tal como la habla la propia app (siempre
    /// contra loopback): `http://127.0.0.1:<puerto>` o, con acceso LAN activo,
    /// `https://127.0.0.1:<puerto>`.
    pub fn url_gitea(&self) -> String {
        match &self.lan {
            Some(_) => format!("https://127.0.0.1:{}", self.puerto),
            None => format!("http://127.0.0.1:{}", self.puerto),
        }
    }

    /// URL que ven las personas (la que Gitea pinta como `ROOT_URL` y en sus URLs de
    /// clonado): igual que [`Cuenta::url_gitea`] sin LAN; con acceso LAN activo, el
    /// nombre `.internal` en vez de `127.0.0.1`, para que sirva tanto al anfitrión (que
    /// también añade esa línea a su `/etc/hosts` apuntando a `127.0.0.1`) como al resto
    /// de la LAN.
    pub fn url_publica(&self) -> String {
        match &self.lan {
            Some(acceso) => format!("https://{}:{}", acceso.host, self.puerto),
            None => format!("http://127.0.0.1:{}", self.puerto),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_acepta_los_validos() {
        // GitHub admite repos que empiezan por punto (`.github`, `.vscode-copilot`).
        for valor in [
            "jparga",
            "fact-mereba",
            "repo.js",
            "a_b",
            "X1",
            ".github",
            ".vscode-copilot",
        ] {
            assert!(Nombre::nuevo(valor).is_ok(), "{valor}");
        }
    }

    #[test]
    fn nombre_rechaza_los_peligrosos() {
        assert_eq!(Nombre::nuevo(""), Err(ErrorNombre::Vacio));
        assert_eq!(
            Nombre::nuevo("a".repeat(101)),
            Err(ErrorNombre::DemasiadoLargo(100))
        );
        for valor in ["a/b", "a b", "a;b", "ñ", "a\n", "$(x)", "a\\b"] {
            assert_eq!(
                Nombre::nuevo(valor),
                Err(ErrorNombre::CaracteresNoPermitidos),
                "{valor:?}"
            );
        }
        for valor in [
            ".",
            "..",
            ".git",
            ".GIT",
            "-rf",
            "a..b",
            "..a",
            "--upload-pack",
        ] {
            assert_eq!(
                Nombre::nuevo(valor),
                Err(ErrorNombre::FormaNoPermitida),
                "{valor:?}"
            );
        }
    }

    #[test]
    fn nombre_se_valida_al_deserializar() {
        assert!(serde_json::from_str::<Nombre>("\"jparga\"").is_ok());
        assert!(serde_json::from_str::<Nombre>("\"../etc\"").is_err());
    }

    #[test]
    fn id_repo_se_muestra_como_dueno_barra_nombre() {
        let id = IdRepo {
            dueno: Nombre::nuevo("jparga").unwrap(),
            nombre: Nombre::nuevo("gitmereba").unwrap(),
        };
        assert_eq!(id.to_string(), "jparga/gitmereba");
    }

    #[test]
    fn estado_ordena_por_urgencia() {
        assert!(EstadoRepo::Fallo < EstadoRepo::Obsoleto);
        assert!(EstadoRepo::Obsoleto < EstadoRepo::Ok);
    }

    fn cuenta_de_prueba(lan: Option<AccesoLan>) -> Cuenta {
        Cuenta {
            login: Nombre::nuevo("jparga").unwrap(),
            carpeta: PathBuf::from("/tmp/x"),
            puerto: 3999,
            intervalo_minutos: 30,
            alcance: Alcance {
                incluir_forks: false,
                organizaciones: vec![],
                excluidos: vec![],
            },
            lan,
        }
    }

    #[test]
    fn la_url_de_gitea_es_siempre_local() {
        assert_eq!(cuenta_de_prueba(None).url_gitea(), "http://127.0.0.1:3999");
    }

    #[test]
    fn con_lan_la_url_de_gitea_sigue_siendo_local_pero_https() {
        let acceso = AccesoLan {
            host: NombreHostInterno::nuevo("jparga.gitmereba.internal").unwrap(),
        };
        assert_eq!(
            cuenta_de_prueba(Some(acceso)).url_gitea(),
            "https://127.0.0.1:3999"
        );
    }

    #[test]
    fn sin_lan_la_url_publica_es_local() {
        assert_eq!(
            cuenta_de_prueba(None).url_publica(),
            "http://127.0.0.1:3999"
        );
    }

    #[test]
    fn con_lan_la_url_publica_usa_el_host_internal() {
        let acceso = AccesoLan {
            host: NombreHostInterno::nuevo("jparga.gitmereba.internal").unwrap(),
        };
        assert_eq!(
            cuenta_de_prueba(Some(acceso)).url_publica(),
            "https://jparga.gitmereba.internal:3999"
        );
    }

    #[test]
    fn un_gitmereba_toml_sin_lan_se_sigue_leyendo() {
        let sin_lan = r#"
            login = "jparga"
            carpeta = "/tmp/x"
            puerto = 3999
            intervalo_minutos = 30

            [alcance]
            incluir_forks = false
            organizaciones = []
            excluidos = []
        "#;
        let cuenta: Cuenta = toml::from_str(sin_lan).expect("un toml sin «lan» se lee igual");
        assert_eq!(cuenta.lan, None);
    }

    #[test]
    fn nombre_host_interno_acepta_hosts_validos() {
        for valor in [
            "jparga.gitmereba.internal",
            "a.internal",
            "a-b-c.internal",
            "uno.dos.internal",
        ] {
            assert!(NombreHostInterno::nuevo(valor).is_ok(), "{valor}");
        }
    }

    #[test]
    fn nombre_host_interno_rechaza_hosts_invalidos() {
        assert_eq!(NombreHostInterno::nuevo(""), Err(ErrorHostInterno::Vacio));
        assert_eq!(
            NombreHostInterno::nuevo("jparga.gitmereba.com"),
            Err(ErrorHostInterno::NoTerminaEnInternal)
        );
        assert_eq!(
            NombreHostInterno::nuevo(".internal"),
            Err(ErrorHostInterno::NoTerminaEnInternal)
        );
        assert_eq!(
            NombreHostInterno::nuevo("internal"),
            Err(ErrorHostInterno::NoTerminaEnInternal)
        );
        assert!(matches!(
            NombreHostInterno::nuevo("Jparga.internal"),
            Err(ErrorHostInterno::EtiquetaInvalida(_))
        ));
        assert!(matches!(
            NombreHostInterno::nuevo("-a.internal"),
            Err(ErrorHostInterno::EtiquetaInvalida(_))
        ));
        assert!(matches!(
            NombreHostInterno::nuevo("a-.internal"),
            Err(ErrorHostInterno::EtiquetaInvalida(_))
        ));
        assert!(matches!(
            NombreHostInterno::nuevo("a_b.internal"),
            Err(ErrorHostInterno::EtiquetaInvalida(_))
        ));
        assert!(matches!(
            NombreHostInterno::nuevo("a..b.internal"),
            Err(ErrorHostInterno::EtiquetaInvalida(_))
        ));
        assert_eq!(
            NombreHostInterno::nuevo(format!("{}.internal", "a".repeat(64))),
            Err(ErrorHostInterno::EtiquetaInvalida("a".repeat(64)))
        );
    }

    #[test]
    fn nombre_host_interno_se_valida_al_deserializar() {
        assert!(serde_json::from_str::<NombreHostInterno>("\"a.internal\"").is_ok());
        assert!(serde_json::from_str::<NombreHostInterno>("\"a.com\"").is_err());
    }

    #[test]
    fn host_lan_por_defecto_sanea_el_login() {
        let login = Nombre::nuevo("JParga").expect("nombre");
        assert_eq!(
            host_lan_por_defecto(&login).as_str(),
            "jparga.gitmereba.internal"
        );
    }

    #[test]
    fn host_lan_por_defecto_sanea_caracteres_no_dns() {
        let login = Nombre::nuevo("_jparga.test_").expect("nombre");
        let host = host_lan_por_defecto(&login);
        // Nunca debe empezar ni acabar en «-», ni contener «_» ni «.» en la primera
        // etiqueta, y siempre termina en «.gitmereba.internal».
        assert!(host.as_str().ends_with(".gitmereba.internal"));
        assert!(NombreHostInterno::nuevo(host.as_str()).is_ok());
    }
}
