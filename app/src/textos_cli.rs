//! Etiquetas fijas de la salida de la CLI (marcadores, consejos, cabeceras de tabla y
//! mensajes de cada comando) en español e inglés.
//!
//! La ayuda de clap vive en `cli_idioma`; los errores y los textos del núcleo ya se
//! localizan en `gitmereba_core::idioma`. Aquí solo lo que escribe la propia CLI.
//! Cada idioma es un `match` exhaustivo, sin brazo comodín: una variante nueva no compila
//! hasta que tiene su texto en los dos.

use gitmereba_core::idioma::Idioma;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextoCli {
    // Sin subcomando.
    SinSubcomando,
    // Marcadores del doctor y consejos.
    MarcaOk,
    MarcaAviso,
    MarcaFallo,
    Consejo(String),
    ConsejoTokenInvalido,
    ConsejoGiteaParado,
    ConsejoCuentaNoExiste,
    ConsejoCarpetaNoVacia,
    ConsejoLoginDuplicado,
    ConsejoRecursoDuplicado,
    ConsejoNadaBorrado,
    // Prefijos de error.
    LlaveroInaccesible(String),
    AlmacenInaccesible(String),
    LlaveroIlegible(String),
    LoginInvalido(String),
    OrganizacionInvalida(String),
    NombreUsuarioInvalido(String),
    HostInvalido(String),
    NoSerializaEstado(String),
    NoLeeCuenta(String),
    NoLeeIndice(String),
    // Cabeceras de tabla y celdas.
    ColLogin,
    ColGitea,
    ColVersion,
    ColRepos,
    ColFallos,
    ColHuerfanos,
    ColUltimaSync,
    ColCarpeta,
    ColPuerto,
    ColIntervalo,
    ColDueno,
    ColTamano,
    Si,
    No,
    Nunca,
    NoHayCuentas,
    // `cuenta add` / `cuenta rm`.
    CarpetaNoAbsoluta,
    TokenNoRecibido,
    PedirToken,
    ConfirmarAlta,
    SufijoSiNo,
    AltaCancelada,
    GiteaPrimerPlano,
    ConfirmarBaja(String),
    BajaCancelada,
    CuentaBaja(String),
    IdentidadConfirmada(String),
    TokenCaduca(String),
    TokenSinCaducidad,
    OrganizacionesDisponibles(String),
    TotalRepos(String, String),
    // `cuenta lan`.
    HostSoloConActivar,
    AccesoLanActivo(String),
    UrlPublica(String),
    HuellaCertificado(String),
    Certificado(String),
    AnadeAHosts,
    LineaHostsLocal(String),
    LineaHostsLan(String),
    SinIpLan,
    GitConfiaEnCertificado,
    ReglaCortafuegos,
    AccesoLanInactivo(String),
    UrlLocal(String),
    CertificadoConservado(String),
    // `cuenta usuario`.
    SinUsuariosLan,
    SinTokenAdmin,
    UsuarioCreado(String),
    /// Solo la etiqueta: la contraseña se añade al imprimir, sin guardarla en un valor `Debug`.
    EtiquetaContrasena,
    NoSeVolveraMostrar,
    UsuarioEliminado(String),
    // `sync`.
    IndicaLoginOTodas,
    LoginYTodas,
    RepoForma,
    CuentaNoExiste(String),
    RepoSincronizacionPedida(String),
    RepoReclonar(String),
}

impl TextoCli {
    pub fn texto(&self, idioma: Idioma) -> String {
        match idioma {
            Idioma::Es => es(self),
            Idioma::En => en(self),
        }
    }
}

fn es(t: &TextoCli) -> String {
    use TextoCli as T;
    match t {
        T::SinSubcomando => "Sin subcomando, gitmereba abre su ventana.".to_string(),
        T::MarcaOk => "[ok]  ".to_string(),
        T::MarcaAviso => "[aviso]".to_string(),
        T::MarcaFallo => "[fallo]".to_string(),
        T::Consejo(c) => format!("consejo: {c}"),
        T::ConsejoTokenInvalido => {
            "comprueba que el token no ha caducado y tiene permiso de lectura".to_string()
        }
        T::ConsejoGiteaParado => {
            "arráncalo con «systemctl --user start» o revisa «gitmereba doctor»".to_string()
        }
        T::ConsejoCuentaNoExiste => {
            "consulta los logins dados de alta con «gitmereba cuenta list»".to_string()
        }
        T::ConsejoCarpetaNoVacia => "elige una carpeta vacía o inexistente".to_string(),
        T::ConsejoLoginDuplicado => {
            "ya hay una cuenta con ese login; usa «gitmereba cuenta list»".to_string()
        }
        T::ConsejoRecursoDuplicado => "esa carpeta o puerto ya los usa otra cuenta".to_string(),
        T::ConsejoNadaBorrado => "no se ha borrado nada; revisa la ruta a mano".to_string(),
        T::LlaveroInaccesible(e) => format!("no se pudo acceder al llavero: {e}"),
        T::AlmacenInaccesible(e) => format!("no se pudo abrir el almacén: {e}"),
        T::LlaveroIlegible(e) => format!("no se pudo leer el llavero: {e}"),
        T::LoginInvalido(e) => format!("login inválido: {e}"),
        T::OrganizacionInvalida(e) => format!("nombre de organización inválido: {e}"),
        T::NombreUsuarioInvalido(e) => format!("nombre de usuario inválido: {e}"),
        T::HostInvalido(e) => format!("host inválido: {e}"),
        T::NoSerializaEstado(e) => format!("no se pudo serializar el estado: {e}"),
        T::NoLeeCuenta(e) => format!("no se pudo leer la cuenta: {e}"),
        T::NoLeeIndice(e) => format!("no se pudo leer el índice de cuentas: {e}"),
        T::ColLogin => "login".to_string(),
        T::ColGitea => "gitea".to_string(),
        T::ColVersion => "versión".to_string(),
        T::ColRepos => "repos".to_string(),
        T::ColFallos => "fallos".to_string(),
        T::ColHuerfanos => "huérfanos".to_string(),
        T::ColUltimaSync => "última sync".to_string(),
        T::ColCarpeta => "carpeta".to_string(),
        T::ColPuerto => "puerto".to_string(),
        T::ColIntervalo => "intervalo (min)".to_string(),
        T::ColDueno => "dueño".to_string(),
        T::ColTamano => "tamaño".to_string(),
        T::Si => "sí".to_string(),
        T::No => "no".to_string(),
        T::Nunca => "nunca".to_string(),
        T::NoHayCuentas => "No hay ninguna cuenta dada de alta.".to_string(),
        T::CarpetaNoAbsoluta => "la carpeta debe ser una ruta absoluta".to_string(),
        T::TokenNoRecibido => "no se ha recibido ningún token por la entrada estándar".to_string(),
        T::PedirToken => "Token de lectura de GitHub: ".to_string(),
        T::ConfirmarAlta => "¿Continuar con el alta?".to_string(),
        T::SufijoSiNo => "[s/N]".to_string(),
        T::AltaCancelada => "Alta cancelada.".to_string(),
        T::GiteaPrimerPlano => {
            "Gitea arrancado en primer plano. Pulsa Ctrl-C para pararlo.".to_string()
        }
        T::ConfirmarBaja(login) => format!("Escribe «{login}» para confirmar la baja: "),
        T::BajaCancelada => "Baja cancelada.".to_string(),
        T::CuentaBaja(login) => format!("Cuenta «{login}» dada de baja."),
        T::IdentidadConfirmada(login) => format!("Identidad confirmada: {login}"),
        T::TokenCaduca(fecha) => format!("El token caduca: {fecha}"),
        T::TokenSinCaducidad => "El token no informa de fecha de caducidad.".to_string(),
        T::OrganizacionesDisponibles(lista) => format!("Organizaciones disponibles: {lista}"),
        T::TotalRepos(repos, kb) => format!("Total: {repos} repo(s), {kb} KB"),
        T::HostSoloConActivar => "«--host» solo tiene sentido junto con «--activar»".to_string(),
        T::AccesoLanActivo(login) => format!("Acceso LAN activo para «{login}»."),
        T::UrlPublica(url) => format!("URL pública: {url}"),
        T::HuellaCertificado(huella) => format!("Huella SHA-256 del certificado: {huella}"),
        T::Certificado(ruta) => format!("Certificado: {ruta}"),
        T::AnadeAHosts => "Añade estas líneas a /etc/hosts:".to_string(),
        T::LineaHostsLocal(host) => format!("  127.0.0.1  {host}   (en este equipo)"),
        T::LineaHostsLan(linea) => format!("  {linea}   (en el resto de equipos de la LAN)"),
        T::SinIpLan => "  (no se pudo determinar la IP de este equipo en la LAN)".to_string(),
        T::GitConfiaEnCertificado => "Para que git confíe en el certificado:".to_string(),
        T::ReglaCortafuegos => "Regla de cortafuegos sugerida:".to_string(),
        T::AccesoLanInactivo(login) => {
            format!("Acceso LAN inactivo para «{login}»: Gitea solo escucha en 127.0.0.1.")
        }
        T::UrlLocal(url) => format!("URL local: {url}"),
        T::CertificadoConservado(huella) => {
            format!("Hay un certificado conservado de una activación anterior (huella {huella}).")
        }
        T::SinUsuariosLan => {
            "No hay usuarios de la LAN: solo el administrador puede entrar.".to_string()
        }
        T::SinTokenAdmin => "no hay token de administración de Gitea para esta cuenta".to_string(),
        T::UsuarioCreado(nombre) => format!("Usuario «{nombre}» creado."),
        T::EtiquetaContrasena => "Contraseña:".to_string(),
        T::NoSeVolveraMostrar => "No se volverá a mostrar. Lee los mirrors y escribe en \
                                  «contingencia-*»; puede cambiarla desde la web de Gitea."
            .to_string(),
        T::UsuarioEliminado(nombre) => format!("Usuario «{nombre}» eliminado."),
        T::IndicaLoginOTodas => "indica un login o usa --todas".to_string(),
        T::LoginYTodas => "indica un login o --todas, no las dos cosas".to_string(),
        T::RepoForma => "--repo debe tener la forma dueño/nombre".to_string(),
        T::CuentaNoExiste(login) => format!("no existe la cuenta «{login}»"),
        T::RepoSincronizacionPedida(id) => format!("«{id}»: sincronización pedida."),
        T::RepoReclonar(id) => {
            format!("«{id}»: el clonado inicial había fallado; se vuelve a clonar.")
        }
    }
}

fn en(t: &TextoCli) -> String {
    use TextoCli as T;
    match t {
        T::SinSubcomando => "With no subcommand, gitmereba opens its window.".to_string(),
        T::MarcaOk => "[ok]  ".to_string(),
        T::MarcaAviso => "[warn]".to_string(),
        T::MarcaFallo => "[fail]".to_string(),
        T::Consejo(c) => format!("hint: {c}"),
        T::ConsejoTokenInvalido => {
            "check that the token has not expired and has read permission".to_string()
        }
        T::ConsejoGiteaParado => {
            "start it with “systemctl --user start” or check “gitmereba doctor”".to_string()
        }
        T::ConsejoCuentaNoExiste => {
            "list the accounts that have been added with “gitmereba cuenta list”".to_string()
        }
        T::ConsejoCarpetaNoVacia => "choose an empty or nonexistent folder".to_string(),
        T::ConsejoLoginDuplicado => {
            "an account with that login already exists; use “gitmereba cuenta list”".to_string()
        }
        T::ConsejoRecursoDuplicado => {
            "that folder or port is already used by another account".to_string()
        }
        T::ConsejoNadaBorrado => "nothing was deleted; check the path by hand".to_string(),
        T::LlaveroInaccesible(e) => format!("could not access the keyring: {e}"),
        T::AlmacenInaccesible(e) => format!("could not open the store: {e}"),
        T::LlaveroIlegible(e) => format!("could not read the keyring: {e}"),
        T::LoginInvalido(e) => format!("invalid login: {e}"),
        T::OrganizacionInvalida(e) => format!("invalid organization name: {e}"),
        T::NombreUsuarioInvalido(e) => format!("invalid user name: {e}"),
        T::HostInvalido(e) => format!("invalid host: {e}"),
        T::NoSerializaEstado(e) => format!("could not serialize the state: {e}"),
        T::NoLeeCuenta(e) => format!("could not read the account: {e}"),
        T::NoLeeIndice(e) => format!("could not read the account index: {e}"),
        T::ColLogin => "login".to_string(),
        T::ColGitea => "gitea".to_string(),
        T::ColVersion => "version".to_string(),
        T::ColRepos => "repos".to_string(),
        T::ColFallos => "failures".to_string(),
        T::ColHuerfanos => "orphans".to_string(),
        T::ColUltimaSync => "last sync".to_string(),
        T::ColCarpeta => "folder".to_string(),
        T::ColPuerto => "port".to_string(),
        T::ColIntervalo => "interval (min)".to_string(),
        T::ColDueno => "owner".to_string(),
        T::ColTamano => "size".to_string(),
        T::Si => "yes".to_string(),
        T::No => "no".to_string(),
        T::Nunca => "never".to_string(),
        T::NoHayCuentas => "No accounts have been added.".to_string(),
        T::CarpetaNoAbsoluta => "the folder must be an absolute path".to_string(),
        T::TokenNoRecibido => "no token was received on standard input".to_string(),
        T::PedirToken => "GitHub read token: ".to_string(),
        T::ConfirmarAlta => "Continue with the account setup?".to_string(),
        T::SufijoSiNo => "[y/N]".to_string(),
        T::AltaCancelada => "Account setup cancelled.".to_string(),
        T::GiteaPrimerPlano => {
            "Gitea started in the foreground. Press Ctrl-C to stop it.".to_string()
        }
        T::ConfirmarBaja(login) => format!("Type “{login}” to confirm the removal: "),
        T::BajaCancelada => "Removal cancelled.".to_string(),
        T::CuentaBaja(login) => format!("Account “{login}” removed."),
        T::IdentidadConfirmada(login) => format!("Identity confirmed: {login}"),
        T::TokenCaduca(fecha) => format!("The token expires: {fecha}"),
        T::TokenSinCaducidad => "The token does not report an expiry date.".to_string(),
        T::OrganizacionesDisponibles(lista) => format!("Available organizations: {lista}"),
        T::TotalRepos(repos, kb) => format!("Total: {repos} repo(s), {kb} KB"),
        T::HostSoloConActivar => "“--host” only makes sense together with “--activar”".to_string(),
        T::AccesoLanActivo(login) => format!("LAN access is on for “{login}”."),
        T::UrlPublica(url) => format!("Public URL: {url}"),
        T::HuellaCertificado(huella) => format!("Certificate SHA-256 fingerprint: {huella}"),
        T::Certificado(ruta) => format!("Certificate: {ruta}"),
        T::AnadeAHosts => "Add these lines to /etc/hosts:".to_string(),
        T::LineaHostsLocal(host) => format!("  127.0.0.1  {host}   (on this machine)"),
        T::LineaHostsLan(linea) => format!("  {linea}   (on the other machines of the LAN)"),
        T::SinIpLan => "  (could not determine this machine's IP on the LAN)".to_string(),
        T::GitConfiaEnCertificado => "To make git trust the certificate:".to_string(),
        T::ReglaCortafuegos => "Suggested firewall rule:".to_string(),
        T::AccesoLanInactivo(login) => {
            format!("LAN access is off for “{login}”: Gitea only listens on 127.0.0.1.")
        }
        T::UrlLocal(url) => format!("Local URL: {url}"),
        T::CertificadoConservado(huella) => format!(
            "A certificate kept from an earlier activation is still there (fingerprint {huella})."
        ),
        T::SinUsuariosLan => {
            "There are no LAN users: only the administrator can sign in.".to_string()
        }
        T::SinTokenAdmin => "there is no Gitea administration token for this account".to_string(),
        T::UsuarioCreado(nombre) => format!("User “{nombre}” created."),
        T::EtiquetaContrasena => "Password:".to_string(),
        T::NoSeVolveraMostrar => "It will not be shown again. The user can read the mirrors and \
                                  write to “contingencia-*”; the password can be changed from \
                                  the Gitea web interface."
            .to_string(),
        T::UsuarioEliminado(nombre) => format!("User “{nombre}” deleted."),
        T::IndicaLoginOTodas => "give a login or use --todas".to_string(),
        T::LoginYTodas => "give a login or --todas, not both".to_string(),
        T::RepoForma => "--repo must have the form owner/name".to_string(),
        T::CuentaNoExiste(login) => format!("account “{login}” does not exist"),
        T::RepoSincronizacionPedida(id) => format!("“{id}”: sync requested."),
        T::RepoReclonar(id) => {
            format!("“{id}”: the initial clone had failed; cloning it again.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadena(texto: &str) -> String {
        texto.to_string()
    }

    /// Una muestra de cada variante, para recorrerlas todas.
    fn todas() -> Vec<TextoCli> {
        use TextoCli as T;
        vec![
            T::SinSubcomando,
            T::MarcaOk,
            T::MarcaAviso,
            T::MarcaFallo,
            T::Consejo(cadena("x")),
            T::ConsejoTokenInvalido,
            T::ConsejoGiteaParado,
            T::ConsejoCuentaNoExiste,
            T::ConsejoCarpetaNoVacia,
            T::ConsejoLoginDuplicado,
            T::ConsejoRecursoDuplicado,
            T::ConsejoNadaBorrado,
            T::LlaveroInaccesible(cadena("x")),
            T::AlmacenInaccesible(cadena("x")),
            T::LlaveroIlegible(cadena("x")),
            T::LoginInvalido(cadena("x")),
            T::OrganizacionInvalida(cadena("x")),
            T::NombreUsuarioInvalido(cadena("x")),
            T::HostInvalido(cadena("x")),
            T::NoSerializaEstado(cadena("x")),
            T::NoLeeCuenta(cadena("x")),
            T::NoLeeIndice(cadena("x")),
            T::ColLogin,
            T::ColGitea,
            T::ColVersion,
            T::ColRepos,
            T::ColFallos,
            T::ColHuerfanos,
            T::ColUltimaSync,
            T::ColCarpeta,
            T::ColPuerto,
            T::ColIntervalo,
            T::ColDueno,
            T::ColTamano,
            T::Si,
            T::No,
            T::Nunca,
            T::NoHayCuentas,
            T::CarpetaNoAbsoluta,
            T::TokenNoRecibido,
            T::PedirToken,
            T::ConfirmarAlta,
            T::SufijoSiNo,
            T::AltaCancelada,
            T::GiteaPrimerPlano,
            T::ConfirmarBaja(cadena("x")),
            T::BajaCancelada,
            T::CuentaBaja(cadena("x")),
            T::IdentidadConfirmada(cadena("x")),
            T::TokenCaduca(cadena("x")),
            T::TokenSinCaducidad,
            T::OrganizacionesDisponibles(cadena("x")),
            T::TotalRepos(cadena("1"), cadena("2")),
            T::HostSoloConActivar,
            T::AccesoLanActivo(cadena("x")),
            T::UrlPublica(cadena("x")),
            T::HuellaCertificado(cadena("x")),
            T::Certificado(cadena("x")),
            T::AnadeAHosts,
            T::LineaHostsLocal(cadena("x")),
            T::LineaHostsLan(cadena("x")),
            T::SinIpLan,
            T::GitConfiaEnCertificado,
            T::ReglaCortafuegos,
            T::AccesoLanInactivo(cadena("x")),
            T::UrlLocal(cadena("x")),
            T::CertificadoConservado(cadena("x")),
            T::SinUsuariosLan,
            T::SinTokenAdmin,
            T::UsuarioCreado(cadena("x")),
            T::EtiquetaContrasena,
            T::NoSeVolveraMostrar,
            T::UsuarioEliminado(cadena("x")),
            T::IndicaLoginOTodas,
            T::LoginYTodas,
            T::RepoForma,
            T::CuentaNoExiste(cadena("x")),
            T::RepoSincronizacionPedida(cadena("x")),
            T::RepoReclonar(cadena("x")),
        ]
    }

    /// Variantes cuyo texto es el mismo en los dos idiomas (palabras comunes).
    fn es_igual_en_ambos(t: &TextoCli) -> bool {
        matches!(
            t,
            TextoCli::MarcaOk
                | TextoCli::ColLogin
                | TextoCli::ColGitea
                | TextoCli::ColRepos
                | TextoCli::No
                | TextoCli::TotalRepos(..)
        )
    }

    #[test]
    fn todo_texto_tiene_contenido_en_los_dos_idiomas() {
        for t in todas() {
            assert!(!t.texto(Idioma::Es).is_empty(), "{t:?} vacío en es");
            assert!(!t.texto(Idioma::En).is_empty(), "{t:?} vacío en en");
        }
    }

    #[test]
    fn el_ingles_difiere_del_espanol_salvo_palabras_comunes() {
        for t in todas() {
            if es_igual_en_ambos(&t) {
                continue;
            }
            assert_ne!(
                t.texto(Idioma::Es),
                t.texto(Idioma::En),
                "{t:?} sin traducir"
            );
        }
    }

    #[test]
    fn el_ingles_no_lleva_signos_ni_vocales_del_espanol() {
        for t in todas() {
            let texto = t.texto(Idioma::En);
            assert!(
                !texto.chars().any(|c| "áéíóúñ¿¡«»".contains(c)),
                "{t:?} en inglés: {texto}"
            );
        }
    }

    #[test]
    fn el_espanol_conserva_el_texto_de_siempre() {
        let es = |t: TextoCli| t.texto(Idioma::Es);
        assert_eq!(es(TextoCli::MarcaOk), "[ok]  ");
        assert_eq!(es(TextoCli::MarcaAviso), "[aviso]");
        assert_eq!(es(TextoCli::MarcaFallo), "[fallo]");
        assert_eq!(es(TextoCli::Consejo(cadena("x"))), "consejo: x");
        assert_eq!(
            es(TextoCli::LoginInvalido(cadena("x"))),
            "login inválido: x"
        );
        assert_eq!(
            es(TextoCli::LlaveroInaccesible(cadena("x"))),
            "no se pudo acceder al llavero: x"
        );
        assert_eq!(
            es(TextoCli::CuentaBaja(cadena("ana"))),
            "Cuenta «ana» dada de baja."
        );
        assert_eq!(es(TextoCli::ColUltimaSync), "última sync");
        assert_eq!(
            es(TextoCli::AccesoLanInactivo(cadena("ana"))),
            "Acceso LAN inactivo para «ana»: Gitea solo escucha en 127.0.0.1."
        );
        assert_eq!(
            es(TextoCli::NoSeVolveraMostrar),
            "No se volverá a mostrar. Lee los mirrors y escribe en «contingencia-*»; \
             puede cambiarla desde la web de Gitea."
        );
    }
}
