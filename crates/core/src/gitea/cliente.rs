//! [`ClienteGitea`]: implementación de [`super::ApiGitea`] contra un Gitea real.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use url::{Host, Url};

use crate::modelo::{IdRepo, Nombre, RepoLocal};
use crate::secretos::Secreto;

use super::ApiGitea;
use super::dto::{self, RespuestaRepo, RespuestaVersion};
use super::equipo::DefinicionEquipo;
use super::error::ErrorGitea;
use super::peticion::PeticionMirror;
use super::saneado;

const TOPE_PAGINAS: u32 = 200;
const TAMANO_PAGINA: &str = "50";

/// Cliente de la API REST v1 de un Gitea local.
#[derive(Debug)]
pub struct ClienteGitea {
    url_base: Url,
    cliente: Client,
    /// Guardado para poder sanear los mensajes de error: nunca se expone.
    token: Secreto,
}

impl ClienteGitea {
    /// Crea el cliente contra un Gitea local por HTTP simple. `url_base` debe ser
    /// `http://127.0.0.1:<puerto>`, `http://localhost:<puerto>` o
    /// `http://[::1]:<puerto>`, sin usuario/clave ni ruta.
    pub fn nuevo(url_base: &str, token: Secreto) -> Result<Self, ErrorGitea> {
        let url = Url::parse(url_base).map_err(|_| ErrorGitea::UrlNoLocal)?;
        validar_url_local(&url, "http")?;
        Self::construir(url, token, None)
    }

    /// Crea el cliente contra un Gitea local expuesto a la LAN por HTTPS.
    /// `url_base` debe ser `https://127.0.0.1:<puerto>`, `https://localhost:<puerto>` o
    /// `https://[::1]:<puerto>` (la app sigue hablando por loopback: nunca por el
    /// nombre `.internal`), sin usuario/clave ni ruta. `certificado_pem` es el
    /// certificado autofirmado de esa cuenta (PEM): es el **único** certificado en el
    /// que confía esta conexión, sin usar las CA del sistema.
    pub fn nuevo_con_certificado(
        url_base: &str,
        token: Secreto,
        certificado_pem: &[u8],
    ) -> Result<Self, ErrorGitea> {
        let url = Url::parse(url_base).map_err(|_| ErrorGitea::UrlNoLocal)?;
        validar_url_local(&url, "https")?;
        Self::construir(url, token, Some(certificado_pem))
    }

    fn construir(
        url: Url,
        token: Secreto,
        certificado_pem: Option<&[u8]>,
    ) -> Result<Self, ErrorGitea> {
        let mut valor_auth =
            HeaderValue::from_str(&format!("token {}", token.exponer())).map_err(|_| {
                ErrorGitea::DatosInvalidos(
                    "el token contiene caracteres no válidos en una cabecera HTTP".to_string(),
                )
            })?;
        valor_auth.set_sensitive(true);
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(AUTHORIZATION, valor_auth);

        let mut constructor = Client::builder()
            .default_headers(cabeceras)
            .user_agent(format!("gitmereba/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy();

        if let Some(pem) = certificado_pem {
            let certificado = reqwest::Certificate::from_pem(pem).map_err(|error| {
                ErrorGitea::DatosInvalidos(format!("certificado inválido: {error}"))
            })?;
            // Confianza acotada a este único certificado: ni las CA del sistema ni
            // ningún otro emisor son válidos para esta conexión.
            constructor = constructor.tls_certs_only([certificado]);
        }

        let cliente = constructor
            .build()
            .map_err(|e| ErrorGitea::RespuestaInesperada {
                estado: 0,
                detalle: e.without_url().to_string(),
            })?;

        Ok(Self {
            url_base: url,
            cliente,
            token,
        })
    }

    /// Construye una URL bajo la base, con segmentos codificados por el crate `url`
    /// (nunca concatenando cadenas).
    fn url_ruta(&self, segmentos: &[&str]) -> Result<Url, ErrorGitea> {
        let mut url = self.url_base.clone();
        {
            let mut partes = url.path_segments_mut().map_err(|()| {
                ErrorGitea::DatosInvalidos("la URL base no admite rutas".to_string())
            })?;
            partes.clear();
            for segmento in segmentos {
                partes.push(segmento);
            }
        }
        Ok(url)
    }

    async fn enviar(&self, peticion: RequestBuilder) -> Result<Response, ErrorGitea> {
        peticion
            .send()
            .await
            .map_err(|e| self.error_de_transporte(e))
    }

    /// Pagina un `GET` sencillo bajo `segmentos`, sin usar `x-total-count` (a diferencia
    /// de [`Self::repos_de`]): se para simplemente al recibir una página vacía.
    async fn paginar_get<T: serde::de::DeserializeOwned>(
        &self,
        segmentos: &[&str],
    ) -> Result<Vec<T>, ErrorGitea> {
        let mut resultado = Vec::new();
        let mut pagina: u32 = 1;

        loop {
            if pagina > TOPE_PAGINAS {
                break;
            }
            let url = self.url_ruta(segmentos)?;
            let pagina_str = pagina.to_string();
            let peticion = self
                .cliente
                .get(url)
                .query(&[("limit", TAMANO_PAGINA), ("page", pagina_str.as_str())]);
            let respuesta = self.enviar(peticion).await?;
            if !respuesta.status().is_success() {
                return Err(self.error_desde_respuesta(respuesta, &[]).await);
            }
            let cuerpo: Vec<T> = respuesta
                .json()
                .await
                .map_err(|e| self.error_de_transporte(e))?;
            if cuerpo.is_empty() {
                break;
            }
            resultado.extend(cuerpo);
            pagina += 1;
        }

        Ok(resultado)
    }

    fn error_de_transporte(&self, error: reqwest::Error) -> ErrorGitea {
        if error.is_connect() || error.is_timeout() {
            return ErrorGitea::NoDisponible;
        }
        let estado = error.status().map(|s| s.as_u16()).unwrap_or(0);
        let detalle = self.sanear_detalle(&error.without_url().to_string(), &[]);
        ErrorGitea::RespuestaInesperada { estado, detalle }
    }

    /// Construye el error a partir de una respuesta no exitosa. Los códigos con
    /// significado propio se traducen a su variante; el resto cae en
    /// `RespuestaInesperada` con el mensaje saneado (sin credenciales ni tokens).
    async fn error_desde_respuesta(&self, respuesta: Response, extra: &[&Secreto]) -> ErrorGitea {
        let estado = respuesta.status();
        match estado {
            StatusCode::UNAUTHORIZED => return ErrorGitea::TokenInvalido,
            StatusCode::FORBIDDEN => return ErrorGitea::SinPermiso,
            StatusCode::NOT_FOUND => return ErrorGitea::NoEncontrado,
            StatusCode::CONFLICT => return ErrorGitea::YaExiste,
            _ => {}
        }
        let cuerpo = respuesta.text().await.unwrap_or_default();
        let mensaje = dto::extraer_mensaje(&cuerpo).unwrap_or(cuerpo);
        let detalle = self.sanear_detalle(&mensaje, extra);
        ErrorGitea::RespuestaInesperada {
            estado: estado.as_u16(),
            detalle,
        }
    }

    /// Oculta credenciales de URL, el token de Gitea y cualquier secreto de `extra`
    /// (p. ej. el token de GitHub usado en `crear_mirror`), y recorta a 500 caracteres.
    fn sanear_detalle(&self, mensaje: &str, extra: &[&Secreto]) -> String {
        let mut texto = saneado::ocultar_credenciales_url(mensaje);
        texto = saneado::ocultar_literal(&texto, self.token.exponer());
        for secreto in extra {
            texto = saneado::ocultar_literal(&texto, secreto.exponer());
        }
        saneado::recortar(&texto)
    }
}

/// La app siempre le habla a Gitea por loopback, sin credenciales ni ruta en la URL,
/// tanto por HTTP simple como (con certificado propio) por HTTPS.
fn validar_url_local(url: &Url, esquema_esperado: &str) -> Result<(), ErrorGitea> {
    let host_local = match url.host() {
        Some(Host::Domain(dominio)) => dominio.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        None => false,
    };
    let sin_credenciales = url.username().is_empty() && url.password().is_none();
    let sin_ruta = matches!(url.path(), "" | "/");
    if url.scheme() == esquema_esperado
        && host_local
        && url.port().is_some()
        && sin_credenciales
        && sin_ruta
    {
        Ok(())
    } else {
        Err(ErrorGitea::UrlNoLocal)
    }
}

/// Valida el formato de un intervalo de mirror: `"0"` o `<n>m`/`<n>h` con mínimo 10 minutos.
fn validar_intervalo(intervalo: &str) -> Result<(), ErrorGitea> {
    if intervalo == "0" {
        return Ok(());
    }
    if intervalo.is_empty() || !intervalo.is_ascii() {
        return Err(ErrorGitea::IntervaloInvalido(intervalo.to_string()));
    }
    let (numero, unidad) = intervalo.split_at(intervalo.len() - 1);
    let cantidad: u32 = numero
        .parse()
        .map_err(|_| ErrorGitea::IntervaloInvalido(intervalo.to_string()))?;
    let valido = match unidad {
        "m" => cantidad >= 10,
        "h" => cantidad >= 1,
        _ => false,
    };
    if valido {
        Ok(())
    } else {
        Err(ErrorGitea::IntervaloInvalido(intervalo.to_string()))
    }
}

impl ApiGitea for ClienteGitea {
    async fn salud(&self) -> Result<bool, ErrorGitea> {
        let url = self.url_ruta(&["api", "healthz"])?;
        match self.cliente.get(url).send().await {
            Ok(respuesta) => Ok(respuesta.status() == StatusCode::OK),
            Err(error) if error.is_connect() || error.is_timeout() => Ok(false),
            Err(error) => Err(self.error_de_transporte(error)),
        }
    }

    async fn version(&self) -> Result<String, ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "version"])?;
        let respuesta = self.enviar(self.cliente.get(url)).await?;
        if !respuesta.status().is_success() {
            return Err(self.error_desde_respuesta(respuesta, &[]).await);
        }
        let cuerpo: RespuestaVersion = respuesta
            .json()
            .await
            .map_err(|e| self.error_de_transporte(e))?;
        Ok(cuerpo.version)
    }

    async fn asegurar_organizacion(&self, nombre: &Nombre) -> Result<(), ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "orgs", nombre.as_str()])?;
        let respuesta = self.enviar(self.cliente.get(url)).await?;
        if respuesta.status().is_success() {
            return Ok(());
        }
        if respuesta.status() != StatusCode::NOT_FOUND {
            return Err(self.error_desde_respuesta(respuesta, &[]).await);
        }

        let url = self.url_ruta(&["api", "v1", "orgs"])?;
        let cuerpo = serde_json::json!({"username": nombre.as_str(), "visibility": "private"});
        let respuesta = self.enviar(self.cliente.post(url).json(&cuerpo)).await?;
        let estado = respuesta.status();
        if estado.is_success()
            || estado == StatusCode::CONFLICT
            || estado == StatusCode::UNPROCESSABLE_ENTITY
        {
            // 409/422: carrera con otro proceso que la creó primero. Idempotente.
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }

    async fn repos_de(&self, dueno: &Nombre) -> Result<Vec<RepoLocal>, ErrorGitea> {
        let mut resultado = Vec::new();
        let mut total: Option<usize> = None;
        let mut pagina: u32 = 1;

        loop {
            if pagina > TOPE_PAGINAS {
                break;
            }
            let url = self.url_ruta(&["api", "v1", "orgs", dueno.as_str(), "repos"])?;
            let pagina_str = pagina.to_string();
            let peticion = self
                .cliente
                .get(url)
                .query(&[("limit", TAMANO_PAGINA), ("page", pagina_str.as_str())]);
            let respuesta = self.enviar(peticion).await?;
            if !respuesta.status().is_success() {
                return Err(self.error_desde_respuesta(respuesta, &[]).await);
            }

            if total.is_none() {
                total = respuesta
                    .headers()
                    .get("x-total-count")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<usize>().ok());
            }

            let cuerpo: Vec<RespuestaRepo> = respuesta
                .json()
                .await
                .map_err(|e| self.error_de_transporte(e))?;
            if cuerpo.is_empty() {
                break;
            }
            for dto in cuerpo {
                resultado.push(dto.a_repo_local()?);
            }

            if let Some(total) = total
                && resultado.len() >= total
            {
                break;
            }
            pagina += 1;
        }

        Ok(resultado)
    }

    async fn repo(&self, id: &IdRepo) -> Result<Option<RepoLocal>, ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "repos", id.dueno.as_str(), id.nombre.as_str()])?;
        let respuesta = self.enviar(self.cliente.get(url)).await?;
        if respuesta.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !respuesta.status().is_success() {
            return Err(self.error_desde_respuesta(respuesta, &[]).await);
        }
        let cuerpo: RespuestaRepo = respuesta
            .json()
            .await
            .map_err(|e| self.error_de_transporte(e))?;
        Ok(Some(cuerpo.a_repo_local()?))
    }

    async fn crear_mirror(&self, peticion: &PeticionMirror) -> Result<RepoLocal, ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "repos", "migrate"])?;
        let cuerpo = dto::cuerpo_migrate(peticion);
        let respuesta = self.enviar(self.cliente.post(url).json(&cuerpo)).await?;
        if respuesta.status() == StatusCode::CONFLICT {
            return Err(ErrorGitea::YaExiste);
        }
        if !respuesta.status().is_success() {
            let extra: Vec<&Secreto> = peticion.token.iter().collect();
            return Err(self.error_desde_respuesta(respuesta, &extra).await);
        }
        let cuerpo: RespuestaRepo = respuesta
            .json()
            .await
            .map_err(|e| self.error_de_transporte(e))?;
        cuerpo.a_repo_local()
    }

    async fn sincronizar_mirror(&self, id: &IdRepo) -> Result<(), ErrorGitea> {
        let url = self.url_ruta(&[
            "api",
            "v1",
            "repos",
            id.dueno.as_str(),
            id.nombre.as_str(),
            "mirror-sync",
        ])?;
        let respuesta = self.enviar(self.cliente.post(url)).await?;
        if respuesta.status().is_success() {
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }

    async fn fijar_intervalo(&self, id: &IdRepo, intervalo: &str) -> Result<(), ErrorGitea> {
        validar_intervalo(intervalo)?;
        let url = self.url_ruta(&["api", "v1", "repos", id.dueno.as_str(), id.nombre.as_str()])?;
        let cuerpo = serde_json::json!({"mirror_interval": intervalo});
        let respuesta = self.enviar(self.cliente.patch(url).json(&cuerpo)).await?;
        if respuesta.status().is_success() {
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }

    async fn crear_repo(
        &self,
        dueno: &Nombre,
        nombre: &Nombre,
        privado: bool,
    ) -> Result<RepoLocal, ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "orgs", dueno.as_str(), "repos"])?;
        let cuerpo = serde_json::json!({
            "name": nombre.as_str(),
            "private": privado,
            "auto_init": false,
        });
        let respuesta = self.enviar(self.cliente.post(url).json(&cuerpo)).await?;
        if !respuesta.status().is_success() {
            return Err(self.error_desde_respuesta(respuesta, &[]).await);
        }
        let cuerpo: RespuestaRepo = respuesta
            .json()
            .await
            .map_err(|e| self.error_de_transporte(e))?;
        cuerpo.a_repo_local()
    }

    async fn borrar_repo(&self, id: &IdRepo) -> Result<(), ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "repos", id.dueno.as_str(), id.nombre.as_str()])?;
        let respuesta = self.enviar(self.cliente.delete(url)).await?;
        if respuesta.status().is_success() || respuesta.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }

    async fn organizaciones(&self) -> Result<Vec<Nombre>, ErrorGitea> {
        let dtos: Vec<dto::RespuestaOrganizacion> =
            self.paginar_get(&["api", "v1", "orgs"]).await?;
        dtos.into_iter()
            .map(|dto| {
                Nombre::nuevo(dto.username)
                    .map_err(|e| ErrorGitea::DatosInvalidos(format!("organización inválida: {e}")))
            })
            .collect()
    }

    async fn buscar_equipo(
        &self,
        org: &Nombre,
        nombre_equipo: &str,
    ) -> Result<Option<u64>, ErrorGitea> {
        let url = self.url_ruta(&["api", "v1", "orgs", org.as_str(), "teams"])?;
        let respuesta = self.enviar(self.cliente.get(url)).await?;
        if !respuesta.status().is_success() {
            return Err(self.error_desde_respuesta(respuesta, &[]).await);
        }
        let equipos: Vec<dto::RespuestaEquipo> = respuesta
            .json()
            .await
            .map_err(|e| self.error_de_transporte(e))?;
        Ok(equipos
            .into_iter()
            .find(|equipo| equipo.name == nombre_equipo)
            .map(|equipo| equipo.id))
    }

    async fn asegurar_equipo(
        &self,
        org: &Nombre,
        equipo: &DefinicionEquipo,
    ) -> Result<u64, ErrorGitea> {
        if let Some(id) = self.buscar_equipo(org, equipo.nombre).await? {
            return Ok(id);
        }

        let url = self.url_ruta(&["api", "v1", "orgs", org.as_str(), "teams"])?;
        let cuerpo = serde_json::json!({
            "name": equipo.nombre,
            "permission": equipo.permiso.como_str(),
            "includes_all_repositories": true,
            "can_create_org_repo": false,
            "units": ["repo.code"],
        });
        let respuesta = self.enviar(self.cliente.post(url).json(&cuerpo)).await?;
        let estado = respuesta.status();
        if estado.is_success() {
            let cuerpo: dto::RespuestaEquipo = respuesta
                .json()
                .await
                .map_err(|e| self.error_de_transporte(e))?;
            return Ok(cuerpo.id);
        }
        if estado == StatusCode::UNPROCESSABLE_ENTITY {
            // Carrera con otro proceso que lo creó primero entre la búsqueda y el POST.
            if let Some(id) = self.buscar_equipo(org, equipo.nombre).await? {
                return Ok(id);
            }
        }
        Err(self.error_desde_respuesta(respuesta, &[]).await)
    }

    async fn miembros_equipo(&self, id: u64) -> Result<Vec<Nombre>, ErrorGitea> {
        let id_str = id.to_string();
        let dtos: Vec<dto::RespuestaMiembroEquipo> = self
            .paginar_get(&["api", "v1", "teams", &id_str, "members"])
            .await?;
        dtos.into_iter()
            .map(|dto| {
                Nombre::nuevo(dto.login).map_err(|e| {
                    ErrorGitea::DatosInvalidos(format!("miembro de equipo inválido: {e}"))
                })
            })
            .collect()
    }

    async fn anadir_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea> {
        let id_str = id.to_string();
        let url = self.url_ruta(&["api", "v1", "teams", &id_str, "members", usuario.as_str()])?;
        let respuesta = self.enviar(self.cliente.put(url)).await?;
        if respuesta.status().is_success() {
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }

    async fn quitar_miembro_equipo(&self, id: u64, usuario: &Nombre) -> Result<(), ErrorGitea> {
        let id_str = id.to_string();
        let url = self.url_ruta(&["api", "v1", "teams", &id_str, "members", usuario.as_str()])?;
        let respuesta = self.enviar(self.cliente.delete(url)).await?;
        if respuesta.status().is_success() || respuesta.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(self.error_desde_respuesta(respuesta, &[]).await)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::gitea::PermisoEquipo;

    fn nombre(v: &str) -> Nombre {
        Nombre::nuevo(v).unwrap()
    }

    fn id(dueno: &str, repo: &str) -> IdRepo {
        IdRepo {
            dueno: nombre(dueno),
            nombre: nombre(repo),
        }
    }

    fn repo_json(nombre: &str, dueno: &str) -> serde_json::Value {
        json!({
            "name": nombre,
            "owner": {"login": dueno},
            "mirror": true,
            "empty": false,
            "private": true,
            "size": 42,
            "mirror_updated": "0001-01-01T00:00:00Z",
        })
    }

    // --- constructor ---------------------------------------------------

    #[test]
    fn el_constructor_acepta_hosts_locales() {
        for url in [
            "http://127.0.0.1:3000",
            "http://localhost:3000",
            "http://[::1]:3000",
        ] {
            assert!(
                ClienteGitea::nuevo(url, Secreto::nuevo("t")).is_ok(),
                "{url} debería aceptarse"
            );
        }
    }

    #[test]
    fn el_constructor_rechaza_hosts_no_locales() {
        for url in ["http://192.168.1.10:3000", "https://git.ejemplo.com"] {
            let resultado = ClienteGitea::nuevo(url, Secreto::nuevo("t"));
            assert!(
                matches!(resultado, Err(ErrorGitea::UrlNoLocal)),
                "{url} debería rechazarse"
            );
        }
    }

    #[test]
    fn nuevo_rechaza_https_incluso_en_un_host_local() {
        let resultado = ClienteGitea::nuevo("https://127.0.0.1:3000", Secreto::nuevo("t"));
        assert!(matches!(resultado, Err(ErrorGitea::UrlNoLocal)));
    }

    // Certificado ECDSA P-256 autofirmado real, generado con «gitea cert --host
    // 127.0.0.1,localhost» (el mismo formato que genera `instancia::certificado` para el
    // acceso LAN), solo para comprobar aquí que `nuevo_con_certificado` lo acepta como
    // certificado de confianza. No es secreto: es la mitad pública de un par de claves
    // desechable, generado solo para este test.
    const CERTIFICADO_DE_PRUEBA: &[u8] = b"-----BEGIN CERTIFICATE-----\n\
MIIBlDCCATqgAwIBAgIRAO0vF7kgK4K1fQyVE0BhUScwCgYIKoZIzj0EAwIwIjEQ\n\
MA4GA1UEChMHQWNtZSBDbzEOMAwGA1UEAxMFR2l0ZWEwHhcNMjYwOTIwMTEwMzMz\n\
WhcNMzYwOTE3MTEwMzMzWjAiMRAwDgYDVQQKEwdBY21lIENvMQ4wDAYDVQQDEwVH\n\
aXRlYTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABJZeXk8E6lM+oM4AhQkQazm9\n\
xAAUjfDBnq6vMK5liJq6hLmjE63k2xr5MkGGA7UkCGLzedmRqjoL3GZyT9jkUdyj\n\
UTBPMA4GA1UdDwEB/wQEAwIFoDATBgNVHSUEDDAKBggrBgEFBQcDATAMBgNVHRMB\n\
Af8EAjAAMBoGA1UdEQQTMBGCCWxvY2FsaG9zdIcEfwAAATAKBggqhkjOPQQDAgNI\n\
ADBFAiEAr0vfZyKNsIF9HOYsXpgHJY7Hd9uGs7QOeBBCdAKyD08CIF2ivskmp+z/\n\
f3AzWxTJHYSlgRoNsZ++90oy+FHtb+g6\n\
-----END CERTIFICATE-----\n";

    #[test]
    fn nuevo_con_certificado_acepta_https_en_un_host_local() {
        let resultado = ClienteGitea::nuevo_con_certificado(
            "https://127.0.0.1:3000",
            Secreto::nuevo("t"),
            CERTIFICADO_DE_PRUEBA,
        );
        assert!(resultado.is_ok(), "{:?}", resultado.err());
    }

    #[test]
    fn nuevo_con_certificado_rechaza_http() {
        let resultado = ClienteGitea::nuevo_con_certificado(
            "http://127.0.0.1:3000",
            Secreto::nuevo("t"),
            CERTIFICADO_DE_PRUEBA,
        );
        assert!(matches!(resultado, Err(ErrorGitea::UrlNoLocal)));
    }

    #[test]
    fn nuevo_con_certificado_rechaza_hosts_no_locales() {
        let resultado = ClienteGitea::nuevo_con_certificado(
            "https://git.ejemplo.com:3000",
            Secreto::nuevo("t"),
            CERTIFICADO_DE_PRUEBA,
        );
        assert!(matches!(resultado, Err(ErrorGitea::UrlNoLocal)));
    }

    #[test]
    fn nuevo_con_certificado_rechaza_un_pem_con_der_corrupto() {
        // Con el backend rustls, `reqwest::Certificate::from_pem` no valida el
        // contenido hasta construir el cliente (guarda los bytes tal cual). Un bloque
        // PEM sin las cabeceras «-----BEGIN CERTIFICATE-----» se ignora en silencio
        // (cero certificados de confianza: la conexión fallaría más tarde, cerrado por
        // defecto, nunca abierto), pero uno con las cabeceras y un DER corrupto sí
        // falla aquí, al construir el cliente.
        let pem_corrupto = b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        let resultado = ClienteGitea::nuevo_con_certificado(
            "https://127.0.0.1:3000",
            Secreto::nuevo("t"),
            pem_corrupto,
        );
        assert!(resultado.is_err(), "un DER corrupto debe rechazarse");
    }

    #[test]
    fn el_constructor_rechaza_urls_con_usuario_o_clave() {
        for url in [
            "http://usuario:clave@127.0.0.1:3000",
            "http://usuario@localhost:3000",
        ] {
            let resultado = ClienteGitea::nuevo(url, Secreto::nuevo("t"));
            assert!(
                matches!(resultado, Err(ErrorGitea::UrlNoLocal)),
                "{url} debería rechazarse"
            );
        }
    }

    #[test]
    fn el_constructor_rechaza_url_con_ruta_o_sin_puerto() {
        for url in ["http://127.0.0.1:3000/base", "http://127.0.0.1"] {
            let resultado = ClienteGitea::nuevo(url, Secreto::nuevo("t"));
            assert!(
                matches!(resultado, Err(ErrorGitea::UrlNoLocal)),
                "{url} debería rechazarse"
            );
        }
    }

    // --- cabeceras -------------------------------------------------------

    #[tokio::test]
    async fn envia_la_cabecera_authorization_con_el_token() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/healthz"))
            .and(header("Authorization", "token el-secreto"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("el-secreto")).unwrap();
        assert!(cliente.salud().await.unwrap());
    }

    // --- salud -------------------------------------------------------------

    #[tokio::test]
    async fn salud_es_verdadera_si_responde_200() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/healthz"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert!(cliente.salud().await.unwrap());
    }

    #[tokio::test]
    async fn salud_es_falsa_si_el_puerto_esta_cerrado() {
        let servidor = MockServer::start().await;
        let url_base = servidor.uri();
        drop(servidor); // el puerto queda cerrado: nadie escucha ya en él.

        let cliente = ClienteGitea::nuevo(&url_base, Secreto::nuevo("t")).unwrap();
        assert!(!cliente.salud().await.unwrap());
    }

    // --- version -------------------------------------------------------------

    #[tokio::test]
    async fn version_lee_el_campo_version() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "1.22.0"})))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert_eq!(cliente.version().await.unwrap(), "1.22.0");
    }

    // --- asegurar_organizacion ------------------------------------------

    #[tokio::test]
    async fn asegurar_organizacion_no_hace_nada_si_ya_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&servidor)
            .await;
        // Sin mock de POST: si se llamara, wiremock fallaría por falta de coincidencia.

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .asegurar_organizacion(&nombre("acme"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn asegurar_organizacion_la_crea_si_no_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/orgs"))
            .and(body_partial_json(
                json!({"username": "acme", "visibility": "private"}),
            ))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .asegurar_organizacion(&nombre("acme"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn asegurar_organizacion_es_idempotente_ante_una_carrera() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/orgs"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "message": "user already exists [name: acme]"
            })))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .asegurar_organizacion(&nombre("acme"))
            .await
            .unwrap();
    }

    // --- repos_de ------------------------------------------------------

    #[tokio::test]
    async fn repos_de_pagina_hasta_agotar_x_total_count() {
        let servidor = MockServer::start().await;
        for pagina in 1..=3u32 {
            let cuerpo: Vec<serde_json::Value> = (0..1)
                .map(|_| repo_json(&format!("repo{pagina}"), "acme"))
                .collect();
            Mock::given(method("GET"))
                .and(path("/api/v1/orgs/acme/repos"))
                .and(query_param("page", pagina.to_string()))
                .and(query_param("limit", "50"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-total-count", "3")
                        .set_body_json(cuerpo),
                )
                .expect(1)
                .mount(&servidor)
                .await;
        }

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let repos = cliente.repos_de(&nombre("acme")).await.unwrap();
        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].id.nombre, nombre("repo1"));
        assert_eq!(repos[2].id.nombre, nombre("repo3"));
        assert_eq!(repos[0].ultima_sync, None); // fecha cero de Gitea.
    }

    #[tokio::test]
    async fn repos_de_para_al_recibir_una_pagina_vacia() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/repos"))
            .and(query_param("page", "1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([repo_json("solo", "acme")])),
            )
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/repos"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let repos = cliente.repos_de(&nombre("acme")).await.unwrap();
        assert_eq!(repos.len(), 1);
    }

    // --- repo ------------------------------------------------------------

    #[tokio::test]
    async fn repo_devuelve_none_si_no_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/acme/demo"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert_eq!(cliente.repo(&id("acme", "demo")).await.unwrap(), None);
    }

    #[tokio::test]
    async fn repo_codifica_bien_nombres_con_caracteres_raros_pero_validos() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/a_b-c/repo.js"))
            .respond_with(ResponseTemplate::new(200).set_body_json(repo_json("repo.js", "a_b-c")))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let repo = cliente.repo(&id("a_b-c", "repo.js")).await.unwrap();
        assert!(repo.is_some());
    }

    // --- crear_mirror ----------------------------------------------------

    #[tokio::test]
    async fn crear_mirror_envia_el_cuerpo_de_migracion_esperado() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/migrate"))
            .and(body_partial_json(json!({
                "clone_addr": "https://github.com/acme/demo.git",
                "repo_owner": "acme",
                "repo_name": "demo",
                "mirror": true,
                "mirror_interval": "30m",
                "private": true,
                "wiki": true,
                "lfs": true,
                "service": "github",
                "auth_token": "ghp_x",
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(repo_json("demo", "acme")))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let peticion = PeticionMirror {
            url_clon: "https://github.com/acme/demo.git".to_string(),
            token: Some(Secreto::nuevo("ghp_x")),
            dueno: nombre("acme"),
            nombre: nombre("demo"),
            intervalo: "30m".to_string(),
            privado: true,
            descripcion: None,
        };
        let repo = cliente.crear_mirror(&peticion).await.unwrap();
        assert_eq!(repo.id, id("acme", "demo"));
    }

    #[tokio::test]
    async fn crear_mirror_devuelve_ya_existe_en_409() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/migrate"))
            .respond_with(ResponseTemplate::new(409))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let peticion = PeticionMirror {
            url_clon: "https://github.com/acme/demo.git".to_string(),
            token: None,
            dueno: nombre("acme"),
            nombre: nombre("demo"),
            intervalo: "30m".to_string(),
            privado: false,
            descripcion: None,
        };
        assert!(matches!(
            cliente.crear_mirror(&peticion).await,
            Err(ErrorGitea::YaExiste)
        ));
    }

    #[tokio::test]
    async fn crear_mirror_con_error_500_no_filtra_el_token_ni_la_url_con_credenciales() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/migrate"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "message": "clone failed: https://x:ghp_SECRETO@github.com/a/b: auth ghp_SECRETO rejected"
            })))
            .mount(&servidor)
            .await;

        let cliente =
            ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("token-de-gitea")).unwrap();
        let peticion = PeticionMirror {
            url_clon: "https://github.com/a/b.git".to_string(),
            token: Some(Secreto::nuevo("ghp_SECRETO")),
            dueno: nombre("a"),
            nombre: nombre("b"),
            intervalo: "30m".to_string(),
            privado: false,
            descripcion: None,
        };
        let error = cliente.crear_mirror(&peticion).await.unwrap_err();
        let texto = format!("{error}");
        assert!(!texto.contains("ghp_SECRETO"), "{texto}");
        assert!(!texto.contains("x:ghp_SECRETO@"), "{texto}");
        let depurado = format!("{error:?}");
        assert!(!depurado.contains("ghp_SECRETO"), "{depurado}");
    }

    // --- sincronizar_mirror / fijar_intervalo -----------------------------

    #[tokio::test]
    async fn sincronizar_mirror_llama_al_endpoint_de_mirror_sync() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/acme/demo/mirror-sync"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .sincronizar_mirror(&id("acme", "demo"))
            .await
            .unwrap();
    }

    #[test]
    fn fijar_intervalo_valida_el_formato() {
        for valido in ["0", "10m", "8h"] {
            assert!(validar_intervalo(valido).is_ok(), "{valido}");
        }
        for invalido in ["5m", "abc", "-1h"] {
            assert!(validar_intervalo(invalido).is_err(), "{invalido}");
        }
    }

    #[tokio::test]
    async fn fijar_intervalo_envia_el_patch_con_el_valor() {
        let servidor = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/v1/repos/acme/demo"))
            .and(body_partial_json(json!({"mirror_interval": "10m"})))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .fijar_intervalo(&id("acme", "demo"), "10m")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fijar_intervalo_rechaza_un_formato_invalido_sin_llamar_a_gitea() {
        let servidor = MockServer::start().await;
        // Sin mocks: si se hiciera una petición, wiremock la rechazaría.

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        assert!(matches!(
            cliente.fijar_intervalo(&id("acme", "demo"), "5m").await,
            Err(ErrorGitea::IntervaloInvalido(_))
        ));
    }

    // --- crear_repo / borrar_repo ------------------------------------------

    #[tokio::test]
    async fn crear_repo_envia_los_datos_de_un_repo_normal_vacio() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/orgs/acme/repos"))
            .and(body_partial_json(json!({
                "name": "demo",
                "private": true,
                "auto_init": false,
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(repo_json("demo", "acme")))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let repo = cliente
            .crear_repo(&nombre("acme"), &nombre("demo"), true)
            .await
            .unwrap();
        assert_eq!(repo.id, id("acme", "demo"));
    }

    #[tokio::test]
    async fn borrar_repo_es_ok_si_no_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/repos/acme/demo"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente.borrar_repo(&id("acme", "demo")).await.unwrap();
    }

    #[tokio::test]
    async fn borrar_repo_llama_al_endpoint_correcto() {
        let servidor = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/repos/acme/demo"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente.borrar_repo(&id("acme", "demo")).await.unwrap();
    }

    // --- organizaciones ----------------------------------------------------

    #[tokio::test]
    async fn organizaciones_pagina_hasta_recibir_una_pagina_vacia() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs"))
            .and(query_param("page", "1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!([{"username": "acme"}, {"username": "contingencia-acme"}]),
                ),
            )
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let orgs = cliente.organizaciones().await.unwrap();
        assert_eq!(orgs, vec![nombre("acme"), nombre("contingencia-acme")]);
    }

    // --- equipos -------------------------------------------------------------

    #[tokio::test]
    async fn buscar_equipo_devuelve_none_si_no_esta_en_la_lista() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([{"id": 1, "name": "Owners"}])),
            )
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let id = cliente
            .buscar_equipo(&nombre("acme"), "lan-lectura")
            .await
            .unwrap();
        assert_eq!(id, None);
    }

    #[tokio::test]
    async fn buscar_equipo_encuentra_por_nombre() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "name": "Owners"},
                {"id": 7, "name": "lan-lectura"},
            ])))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let id = cliente
            .buscar_equipo(&nombre("acme"), "lan-lectura")
            .await
            .unwrap();
        assert_eq!(id, Some(7));
    }

    #[tokio::test]
    async fn asegurar_equipo_lo_crea_si_no_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/orgs/acme/teams"))
            .and(body_partial_json(json!({
                "name": "lan-lectura",
                "permission": "read",
                "includes_all_repositories": true,
                "can_create_org_repo": false,
            })))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(json!({"id": 9, "name": "lan-lectura"})),
            )
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let equipo = DefinicionEquipo {
            nombre: "lan-lectura",
            permiso: PermisoEquipo::Lectura,
        };
        let id = cliente
            .asegurar_equipo(&nombre("acme"), &equipo)
            .await
            .unwrap();
        assert_eq!(id, 9);
    }

    #[tokio::test]
    async fn asegurar_equipo_no_lo_recrea_si_ya_existe() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"id": 3, "name": "lan-escritura"}])),
            )
            .mount(&servidor)
            .await;
        // Sin mock de POST: si se llamara, wiremock fallaría por falta de coincidencia.

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let equipo = DefinicionEquipo {
            nombre: "lan-escritura",
            permiso: PermisoEquipo::Escritura,
        };
        let id = cliente
            .asegurar_equipo(&nombre("acme"), &equipo)
            .await
            .unwrap();
        assert_eq!(id, 3);
    }

    #[tokio::test]
    async fn asegurar_equipo_es_idempotente_ante_una_carrera() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/orgs/acme/teams"))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(json!({"message": "team already exists"})),
            )
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let equipo = DefinicionEquipo {
            nombre: "lan-lectura",
            permiso: PermisoEquipo::Lectura,
        };
        // La segunda «búsqueda» (tras el 422) también devuelve un array vacío en este
        // mock: no hay forma de simular la carrera exacta con wiremock estático, así
        // que se comprueba que el error se propaga en vez de entrar en pánico, con
        // datos correctos.
        let resultado = cliente.asegurar_equipo(&nombre("acme"), &equipo).await;
        assert!(resultado.is_err());
    }

    #[tokio::test]
    async fn miembros_equipo_pagina_hasta_recibir_una_pagina_vacia() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/teams/9/members"))
            .and(query_param("page", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"login": "ana"}, {"login": "bea"}])),
            )
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/teams/9/members"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        let miembros = cliente.miembros_equipo(9).await.unwrap();
        assert_eq!(miembros, vec![nombre("ana"), nombre("bea")]);
    }

    #[tokio::test]
    async fn anadir_miembro_equipo_llama_al_endpoint_correcto() {
        let servidor = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/teams/9/members/ana"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .anadir_miembro_equipo(9, &nombre("ana"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn quitar_miembro_equipo_es_ok_si_no_era_miembro() {
        let servidor = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/teams/9/members/ana"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;

        let cliente = ClienteGitea::nuevo(&servidor.uri(), Secreto::nuevo("t")).unwrap();
        cliente
            .quitar_miembro_equipo(9, &nombre("ana"))
            .await
            .unwrap();
    }
}
