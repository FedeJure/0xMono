//! La página de configuración que se abre en el celular: qué responder a cada
//! pedido, el formulario y cómo se convierte en un [`Config`].

use core::fmt::{self, Write};

use heapless::String;

use crate::address;
use crate::config::{
    self, Config, DEFAULT_POOL_HOST, DEFAULT_POOL_PORT, DEFAULT_WORKER, Pass, Ssid, Worker, text,
};
use crate::http::{self, Method, Request};

pub use captive::{AP_HOST, AP_IP, AP_URL, BufWriter, Network, REDIRECT, add_network, head};
use captive::{Esc, bars};

const SAVE_PATH: &str = "/guardar";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Page,
    Save,
    Redirect,
}

pub fn route(req: &Request) -> Route {
    // Los chequeos de "¿hay internet?" de Android, iPhone y Windows piden
    // otros dominios: redirigirlos es lo que hace aparecer la página sola.
    if !req.host.is_empty() && req.host != AP_HOST {
        return Route::Redirect;
    }
    match (req.method, req.path) {
        (Method::Get, "/") => Route::Page,
        (Method::Post, SAVE_PATH) => Route::Save,
        _ => Route::Redirect,
    }
}

// --- Formulario ----------------------------------------------------------------

/// Lo que muestra (y devuelve) el formulario.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    pub ssid: Ssid,
    pub pass: Pass,
    /// Más largo que [`config::Address`] para reconocer (y rechazar) una frase secreta
    /// o una clave privada pegada por error.
    pub address: String<256>,
    /// `servidor:puerto`.
    pub pool: String<80>,
    pub worker: Worker,
}

impl Form {
    /// Formulario de un aparato nuevo.
    pub fn new() -> Form {
        let mut pool = String::new();
        let _ = write!(pool, "{DEFAULT_POOL_HOST}:{DEFAULT_POOL_PORT}");
        Form {
            pool,
            worker: text(DEFAULT_WORKER).unwrap_or_default(),
            ..Form::default()
        }
    }

    /// Precargado con lo guardado. La clave del WiFi no se muestra: vacía
    /// quiere decir "la misma de antes".
    pub fn from_config(c: &Config) -> Form {
        let mut pool = String::new();
        let _ = write!(pool, "{}:{}", c.pool_host, c.pool_port);
        Form {
            ssid: c.ssid.clone(),
            pass: Pass::new(),
            address: text(&c.address).unwrap_or_default(),
            pool,
            worker: c.worker.clone(),
        }
    }

    /// Lee el cuerpo `application/x-www-form-urlencoded` del POST.
    pub fn parse(body: &str) -> Result<Form, &'static str> {
        fn get<const N: usize>(
            body: &str,
            key: &str,
            err: &'static str,
        ) -> Result<String<N>, &'static str> {
            http::decode(http::field(body, key).unwrap_or("")).ok_or(err)
        }
        let mut ssid: Ssid = get(body, "red", "El nombre de la red es demasiado largo.")?;
        if ssid.is_empty() {
            ssid = get(
                body,
                "otra",
                "El nombre de la red es demasiado largo (máximo 32).",
            )?;
        }
        Ok(Form {
            ssid,
            pass: get(
                body,
                "clave",
                "La clave del WiFi es demasiado larga (máximo 63).",
            )?,
            address: get(body, "direccion", "La dirección es demasiado larga.")?,
            pool: get(body, "pool", "El pool es demasiado largo.")?,
            worker: get(
                body,
                "nombre",
                "El nombre del minero es demasiado largo (máximo 32).",
            )?,
        })
    }

    /// Valida todo y arma la configuración a guardar. `current` es la que ya
    /// estaba, para conservar la clave si la dejó vacía.
    pub fn to_config(&self, current: Option<&Config>) -> Result<Config, &'static str> {
        let ssid = self.ssid.trim();
        if ssid.is_empty() {
            return Err("Elegí tu red WiFi (o escribí su nombre).");
        }
        let ssid: Ssid = text(ssid).ok_or("El nombre de la red es demasiado largo.")?;

        let pass = match current {
            Some(c) if self.pass.is_empty() && c.ssid == ssid => c.pass.clone(),
            _ => self.pass.clone(),
        };
        let hex_key = pass.len() == 64 && pass.bytes().all(|b| b.is_ascii_hexdigit());
        if !pass.is_empty() && !(8..=63).contains(&pass.len()) && !hex_key {
            return Err("La clave del WiFi tiene que tener entre 8 y 63 caracteres.");
        }

        let addr = self.address.trim();
        if let Some(warning) = secret_warning(addr) {
            return Err(warning);
        }
        address::check(addr).map_err(|e| e.message())?;

        let (pool_host, pool_port) = if self.pool.trim().is_empty() {
            (
                text(DEFAULT_POOL_HOST).unwrap_or_default(),
                DEFAULT_POOL_PORT,
            )
        } else {
            parse_pool(&self.pool)
                .ok_or("El pool va como servidor:puerto, por ejemplo public-pool.io:21496.")?
        };

        let worker = self.worker.trim();
        let worker = if worker.is_empty() {
            DEFAULT_WORKER
        } else {
            worker
        };
        if !worker
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("El nombre del minero solo puede tener letras, números, - y _.");
        }

        Ok(Config {
            ssid,
            pass,
            address: text(addr).ok_or("La dirección es demasiado larga.")?,
            pool_host,
            pool_port,
            worker: text(worker).ok_or("El nombre del minero es demasiado largo.")?,
        })
    }
}

fn parse_pool(pool: &str) -> Option<(config::Host, u16)> {
    let pool = pool.trim();
    let pool = pool.strip_prefix("stratum+tcp://").unwrap_or(pool);
    let (host, port) = pool.rsplit_once(':')?;
    let ok_host = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let port: u16 = port.parse().ok()?;
    (ok_host && port != 0).then_some(())?;
    Some((text(host)?, port))
}

/// Si lo que pegó parece algo secreto en vez de una dirección, avisa fuerte
/// y no se guarda nada.
fn secret_warning(s: &str) -> Option<&'static str> {
    let words = s.split_whitespace().count();
    let b58 = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() && !b"0OIl".contains(&b))
    };
    if words >= 11 {
        Some(
            "¡Eso parece tu frase secreta (las 12 o 24 palabras)! No la escribas en ningún lado: no se guardó. Poné solo una dirección para recibir.",
        )
    } else if (s.len() == 51 || s.len() == 52) && s.starts_with(['5', 'K', 'L']) && b58(s) {
        Some("¡Eso parece una clave privada! No se guardó. Poné solo una dirección para recibir.")
    } else if s.len() > 100 && ["xprv", "yprv", "zprv"].iter().any(|p| s.starts_with(p)) {
        Some(
            "¡Eso es una clave privada extendida! No se guardó. Poné solo una dirección para recibir.",
        )
    } else if s.len() > 100 && ["xpub", "ypub", "zpub"].iter().any(|p| s.starts_with(p)) {
        Some(
            "Eso es una clave pública extendida, no una dirección. En tu billetera buscá \"Recibir\" y copiá la dirección.",
        )
    } else {
        None
    }
}

// --- HTML ----------------------------------------------------------------------

const STYLE: &str = "\
:root{--bg:#14110f;--card:#1f1a17;--ink:#f3ece4;--mute:#a89d92;--acc:#f7931a;--err:#ff6b5b;--line:#3a322c}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.45 system-ui,sans-serif}\
main{max-width:30rem;margin:0 auto;padding:1.25rem 1rem 2rem}h1{margin:.2rem 0;font-size:1.6rem}\
.sub,.help,.foot{color:var(--mute)}.help{font-size:.9rem;margin:.2rem 0 .6rem}.foot{font-size:.85rem;margin-top:1.5rem}\
fieldset{min-width:0;border:1px solid var(--line);border-radius:12px;background:var(--card);margin:1rem 0;padding:.8rem 1rem 1rem}\
legend{color:var(--acc);font-weight:700;padding:0 .3rem}\
label{display:block;margin:.5rem 0}input:not([type=radio]):not([type=checkbox]){display:block;width:100%;margin-top:.25rem;\
padding:.7rem;border-radius:8px;border:1px solid var(--line);background:#0e0c0b;color:var(--ink);font:inherit}\
.net{display:flex;align-items:center;gap:.6rem;padding:.55rem .2rem;margin:0;border-bottom:1px solid var(--line)}\
.net span{flex:1;overflow-wrap:anywhere}.net i{color:var(--mute);font-style:normal;font-size:.85rem;white-space:nowrap}\
input[type=radio],input[type=checkbox]{width:1.2rem;height:1.2rem;accent-color:var(--acc)}\
.chk{display:flex;gap:.5rem;align-items:center;color:var(--mute);font-size:.9rem}\
details{margin:1rem 0;color:var(--mute)}summary{cursor:pointer;padding:.3rem 0}\
button{width:100%;padding:.9rem;border:0;border-radius:10px;background:var(--acc);color:#1a0f00;font:700 1.05rem system-ui,sans-serif}\
.err{background:#3a1512;border:1px solid var(--err);color:#ffd9d4;border-radius:10px;padding:.7rem .9rem}";

fn page_start(w: &mut impl Write) -> fmt::Result {
    write!(
        w,
        "<!doctype html><html lang=es><head><meta charset=utf-8>\
<meta name=viewport content=\"width=device-width,initial-scale=1\">\
<title>Mono minero</title><style>{STYLE}</style></head><body><main>"
    )
}

/// El formulario. `error` se muestra arriba si el envío anterior no sirvió.
pub fn page(
    w: &mut impl Write,
    networks: &[Network],
    form: &Form,
    error: Option<&str>,
    has_config: bool,
) -> fmt::Result {
    page_start(w)?;
    w.write_str("<h1>⛏️ Mono minero</h1>")?;
    if has_config {
        w.write_str("<p class=sub>Cambiá lo que necesites y guardá.</p>")?;
    } else {
        w.write_str(
            "<p class=sub>Conectá al mono a tu WiFi y decile a dónde mandar el premio.</p>",
        )?;
    }
    if let Some(e) = error {
        write!(w, "<p class=err>{}</p>", Esc(e))?;
    }

    write!(
        w,
        "<form method=post action={SAVE_PATH}><fieldset><legend>1. Tu WiFi</legend><p class=help>El mono solo ve redes de 2,4 GHz.</p>"
    )?;
    let listed = networks.iter().any(|n| n.ssid == form.ssid);
    for n in networks {
        write!(
            w,
            "<label class=net><input type=radio name=red value=\"{0}\"{1}><span>{0}</span><i>{2}{3}</i></label>",
            Esc(&n.ssid),
            if n.ssid == form.ssid { " checked" } else { "" },
            if n.open { "abierta " } else { "🔒 " },
            bars(n.rssi),
        )?;
    }
    write!(
        w,
        "<label class=net><input type=radio name=red value=\"\"{}><span>Otra red (oculta o que no aparece):</span></label>\
<input name=otra value=\"{}\" placeholder=\"Nombre exacto de la red\" autocapitalize=off autocorrect=off spellcheck=false>",
        if listed && !form.ssid.is_empty() {
            ""
        } else {
            " checked"
        },
        if listed { "" } else { form.ssid.as_str() },
    )?;
    write!(
        w,
        "<label>Clave del WiFi<input type=password name=clave id=clave value=\"{}\" placeholder=\"{}\" \
autocomplete=off autocapitalize=off autocorrect=off spellcheck=false></label>\
<label class=chk><input type=checkbox onclick=\"clave.type=this.checked?'text':'password'\">Mostrar la clave</label>\
</fieldset>",
        Esc(&form.pass),
        if has_config {
            "Vacía = la misma de antes"
        } else {
            "Vacía si la red es abierta"
        },
    )?;

    write!(
        w,
        "<fieldset><legend>2. Tu dirección de Bitcoin</legend>\
<p class=help>La que usás para <b>recibir</b> (empieza con bc1, 1 o 3). Si el mono encuentra un bloque, el premio va ahí. \
<b>Nunca</b> pongas tu clave privada ni tus 12/24 palabras.</p>\
<input name=direccion value=\"{}\" placeholder=\"bc1q...\" autocapitalize=off autocorrect=off spellcheck=false></fieldset>",
        Esc(&form.address),
    )?;

    write!(
        w,
        "<details><summary>Opciones avanzadas</summary>\
<label>Pool (servidor:puerto)<input name=pool value=\"{}\" autocapitalize=off autocorrect=off spellcheck=false></label>\
<label>Nombre del minero<input name=nombre value=\"{}\" autocapitalize=off autocorrect=off spellcheck=false></label>\
</details><button>Guardar y empezar a minar</button></form>\
<p class=foot>Minar con el mono es una lotería por diversión: la chance de ganar un bloque es casi nula. \
Tu dirección es pública y la clave del WiFi queda guardada solo en el mono.</p></main></body></html>",
        Esc(&form.pool),
        Esc(&form.worker),
    )
}

/// Respuesta después de guardar.
pub fn saved_page(w: &mut impl Write, ssid: &str) -> fmt::Result {
    page_start(w)?;
    write!(
        w,
        "<h1>¡Listo! 🐒</h1><p>El mono se va a conectar a <b>{}</b> y se pone a minar.</p>\
<p class=sub>Ya podés cerrar esta página: tu celular vuelve solo a su WiFi.</p></main></body></html>",
        Esc(ssid)
    )
}

/// Atiende un pedido completo (`body_in` es su cuerpo): escribe la cabecera
/// de la respuesta en `head` y el HTML en `body`. Si el formulario quedó bien,
/// devuelve la configuración para guardar.
///
/// `notice` es un aviso para mostrar arriba de la página (por ejemplo, por qué
/// falló la conexión la vez anterior).
pub fn respond(
    req: &Request,
    body_in: &[u8],
    networks: &[Network],
    current: Option<&Config>,
    notice: Option<&str>,
    head: &mut impl Write,
    body: &mut BufWriter,
) -> Result<Option<Config>, fmt::Error> {
    let blank = || current.map_or_else(Form::new, Form::from_config);
    let saved = match route(req) {
        Route::Redirect => {
            head.write_str(REDIRECT)?;
            return Ok(None);
        }
        Route::Page => {
            page(body, networks, &blank(), notice, current.is_some())?;
            None
        }
        Route::Save => {
            let form = core::str::from_utf8(body_in)
                .map_err(|_| "El formulario llegó mal, probá de nuevo.")
                .and_then(Form::parse);
            match form {
                Err(e) => {
                    page(body, networks, &blank(), Some(e), current.is_some())?;
                    None
                }
                Ok(form) => match form.to_config(current) {
                    Err(e) => {
                        page(body, networks, &form, Some(e), current.is_some())?;
                        None
                    }
                    Ok(config) => {
                        saved_page(body, &config.ssid)?;
                        Some(config)
                    }
                },
            }
        }
    };
    self::head(head, body.len())?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;

    const ADDR: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    fn req(method: Method, host: &'static str, path: &'static str) -> Request<'static> {
        Request {
            method,
            path,
            query: "",
            host,
            content_length: 0,
            body_start: 0,
        }
    }

    #[test]
    fn captive_checks_are_redirected() {
        assert_eq!(
            route(&req(
                Method::Get,
                "connectivitycheck.gstatic.com",
                "/generate_204"
            )),
            Route::Redirect
        );
        assert_eq!(
            route(&req(
                Method::Get,
                "captive.apple.com",
                "/hotspot-detect.html"
            )),
            Route::Redirect
        );
        assert_eq!(route(&req(Method::Get, AP_HOST, "/")), Route::Page);
        assert_eq!(route(&req(Method::Get, "", "/")), Route::Page);
        assert_eq!(route(&req(Method::Post, AP_HOST, "/guardar")), Route::Save);
        assert_eq!(
            route(&req(Method::Get, AP_HOST, "/favicon.ico")),
            Route::Redirect
        );
    }

    #[test]
    fn form_round_trip_to_config() {
        let body = "red=&otra=WiFi+de+casa&clave=clave-secreta&direccion=+bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4+&pool=stratum%2Btcp%3A%2F%2Fsolo.ckpool.org%3A3333&nombre=";
        let c = Form::parse(body).unwrap().to_config(None).unwrap();
        assert_eq!(c.ssid, "WiFi de casa");
        assert_eq!(c.pass, "clave-secreta");
        assert_eq!(c.address, ADDR);
        assert_eq!(
            (c.pool_host.as_str(), c.pool_port),
            ("solo.ckpool.org", 3333)
        );
        assert_eq!(c.worker, DEFAULT_WORKER);
    }

    #[test]
    fn empty_password_keeps_the_saved_one_for_the_same_network() {
        let mut form = Form::new();
        form.ssid = text("Casa").unwrap();
        form.pass = text("clave-vieja").unwrap();
        form.address = text(ADDR).unwrap();
        let saved = form.to_config(None).unwrap();

        let edit = Form::from_config(&saved);
        assert!(edit.pass.is_empty());
        assert_eq!(edit.to_config(Some(&saved)).unwrap().pass, "clave-vieja");

        let mut other = edit.clone();
        other.ssid = text("Otra").unwrap();
        assert_eq!(other.to_config(Some(&saved)).unwrap().pass, "");
    }

    #[test]
    fn friendly_errors() {
        let mut f = Form::new();
        f.address = text(ADDR).unwrap();
        assert!(f.to_config(None).unwrap_err().contains("Elegí tu red"));

        f.ssid = text("Casa").unwrap();
        f.pass = text("corta").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("entre 8 y 63"));

        f.pass = Pass::new();
        f.address = text("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("código de control"));

        f.address = text(ADDR).unwrap();
        f.pool = text("public-pool.io").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("servidor:puerto"));

        f.pool = text("public-pool.io:21496").unwrap();
        f.worker = text("mono feliz").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("letras, números"));
    }

    #[test]
    fn secrets_are_refused() {
        let mut f = Form::new();
        f.ssid = text("Casa").unwrap();
        f.address = text("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("frase secreta"));
        f.address = text("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ").unwrap();
        assert!(f.to_config(None).unwrap_err().contains("clave privada"));
    }

    #[test]
    fn respond_saves_a_good_form_and_explains_a_bad_one() {
        let networks = [Network {
            ssid: text("Casa").unwrap(),
            rssi: -50,
            open: false,
        }];
        let mut head_buf = [0u8; 256];
        let mut body_buf = [0u8; 12 * 1024];

        let mut post = |form: &str| {
            let raw = std::format!(
                "POST /guardar HTTP/1.1\r\nHost: 192.168.4.1\r\nContent-Length: {}\r\n\r\n{form}",
                form.len()
            );
            let req = http::parse(raw.as_bytes()).unwrap();
            let body_in = &raw.as_bytes()[req.body_start..];
            let mut head = BufWriter::new(&mut head_buf);
            let mut body = BufWriter::new(&mut body_buf);
            let saved =
                respond(&req, body_in, &networks, None, None, &mut head, &mut body).unwrap();
            let head = std::string::String::from_utf8(head.as_bytes().to_vec()).unwrap();
            let body = std::string::String::from_utf8(body.as_bytes().to_vec()).unwrap();
            (saved, head, body)
        };

        let (saved, head, body) = post(&std::format!("red=Casa&clave=12345678&direccion={ADDR}"));
        assert_eq!(saved.unwrap().ssid, "Casa");
        assert!(head.contains(&std::format!("Content-Length: {}", body.len())));
        assert!(body.contains("¡Listo!"));

        let (saved, _, body) = post("red=Casa&clave=12345678&direccion=bc1qnada");
        assert!(saved.is_none());
        assert!(body.contains("class=err"));
        assert!(
            body.contains("value=\"bc1qnada\""),
            "conserva lo que escribió"
        );
    }

    #[test]
    fn page_escapes_and_fits() {
        let mut networks: Vec<Network, 16> = Vec::new();
        for i in 0..16 {
            let mut name: String<32> = String::new();
            let _ = write!(name, "Red \"<{i}>\" con nombre largo xx");
            add_network(&mut networks, &name, -40 - i as i8 * 3, i % 3 == 0);
        }
        let mut form = Form::new();
        form.address = text("<script>").unwrap();
        let mut buf = [0u8; 12 * 1024];
        let mut w = BufWriter::new(&mut buf);
        page(&mut w, &networks, &form, Some("a < b"), false).unwrap();
        let html = core::str::from_utf8(w.as_bytes()).unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("a &lt; b"));
        assert!(w.len() < 10 * 1024, "{} bytes", w.len());
    }
}
