//! La página de configuración que se abre en el celular: qué responder a cada
//! pedido, el formulario, cómo se convierte en [`Settings`] y qué hacer
//! después ([`Outcome::next`]).
//!
//! Todo va en la misma respuesta (estilos, el mono en SVG y un poquito de
//! JavaScript): el celular está conectado al mono y no tiene internet.

use core::fmt::{self, Write};

use captive::http::{self, Method, Request};
use captive::{AP_HOST, BufWriter, Esc, Network, Pass, Ssid, bars, text, valid_pass};
use companion::LocalTime;
use heapless::String;

use crate::date::{self, Clock, IsoDate};
use crate::handoff::{HouseStart, Next, Notice, SyncStart};
use crate::tz::{self, Offset, ZONES};
use crate::{DEFAULT_UTC_OFFSET_MIN, Settings};

pub const SAVE_PATH: &str = "/guardar";
pub const NEW_MONKEY_PATH: &str = "/mono-nuevo";
pub const FACTORY_PATH: &str = "/de-fabrica";
/// Valor del botón "sin WiFi" en la lista de redes.
const NO_WIFI: &str = "*sin-wifi*";

/// Lugar que necesita la página más grande (con 16 redes de nombre largo).
pub const MAX_PAGE: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Page,
    Save,
    NewMonkey,
    Factory,
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
        (Method::Post, NEW_MONKEY_PATH) => Route::NewMonkey,
        (Method::Post, FACTORY_PATH) => Route::Factory,
        _ => Route::Redirect,
    }
}

/// Lo que ve la página. No cambia mientras dura el modo configuración.
pub struct Context<'a> {
    /// Las redes que encontró, de la más fuerte a la más débil.
    pub networks: &'a [Network],
    pub current: &'a Settings,
    /// Un aviso para mostrar arriba.
    pub notice: Option<&'a str>,
    /// La hora que tiene el mono, para precargar la fecha sin JavaScript.
    pub now: Option<LocalTime>,
    pub version: &'a str,
}

/// Qué se eligió en la página.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Guardar la configuración. `clock` es la hora puesta a mano o, con la
    /// automática, la del celular (por si internet falla).
    Saved {
        settings: Settings,
        clock: Option<LocalTime>,
    },
    /// Olvidar al personaje: llega otro, con otra semilla.
    NewMonkey { clock: Option<LocalTime> },
    /// Borrar la configuración y el personaje.
    Factory { clock: Option<LocalTime> },
}

impl Outcome {
    /// Qué hace el próximo arranque.
    pub fn next(&self) -> Next {
        match self {
            Outcome::Saved { settings, clock } if settings.wants_sync() => Next::Sync(SyncStart {
                manual: true,
                new_monkey: false,
                fallback: *clock,
            }),
            Outcome::Saved { clock, .. } => Next::House(HouseStart {
                clock: *clock,
                new_monkey: false,
                notice: Some(Notice::Saved),
            }),
            Outcome::NewMonkey { clock } => Next::House(HouseStart {
                clock: *clock,
                new_monkey: true,
                notice: Some(Notice::NewMonkey),
            }),
            Outcome::Factory { clock } => Next::House(HouseStart {
                clock: *clock,
                new_monkey: true,
                notice: Some(Notice::Factory),
            }),
        }
    }
}

// --- Formulario ----------------------------------------------------------------

/// Lo que muestra (y devuelve) el formulario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// La red elegida de la lista, vacía para "otra" o [`NO_WIFI`].
    pub choice: Ssid,
    /// El nombre escrito a mano ("otra red").
    pub other: Ssid,
    pub pass: Pass,
    pub auto_time: bool,
    pub zone: i16,
    pub date: String<16>,
    pub time: String<8>,
}

impl Form {
    /// Precargado con lo guardado. La clave no se muestra: vacía quiere decir
    /// "la misma de antes".
    pub fn from_settings(s: &Settings) -> Form {
        Form {
            choice: if s.has_wifi() {
                s.ssid.clone()
            } else {
                Ssid::new()
            },
            other: Ssid::new(),
            pass: Pass::new(),
            auto_time: s.auto_time,
            zone: s.utc_offset_min,
            date: String::new(),
            time: String::new(),
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
        let zone: String<8> = get(body, "zona", "La zona horaria llegó mal.")?;
        let mode: String<8> = get(body, "modo", "El formulario llegó mal.")?;
        Ok(Form {
            choice: get(body, "red", "El nombre de la red es demasiado largo.")?,
            other: get(
                body,
                "otra",
                "El nombre de la red es demasiado largo (máximo 32 letras).",
            )?,
            pass: get(
                body,
                "clave",
                "La clave del WiFi es demasiado larga (máximo 63).",
            )?,
            auto_time: mode != "mano",
            zone: tz::parse_offset(&zone).unwrap_or(DEFAULT_UTC_OFFSET_MIN),
            date: get(body, "fecha", "La fecha llegó mal.")?,
            time: get(body, "hora", "La hora llegó mal.")?,
        })
    }

    fn no_wifi(&self) -> bool {
        self.choice == NO_WIFI
    }

    /// La hora escrita (o la del celular), si está y es válida.
    pub fn clock(&self) -> Option<LocalTime> {
        date::local_time(&self.date, &self.time)
    }

    /// Valida todo y arma la configuración a guardar, con la hora a poner.
    /// `current` es la que ya estaba, para conservar la clave si la dejó vacía.
    pub fn to_settings(
        &self,
        current: &Settings,
    ) -> Result<(Settings, Option<LocalTime>), &'static str> {
        let (ssid, pass) = if self.no_wifi() {
            (Ssid::new(), Pass::new())
        } else {
            let name = if self.choice.is_empty() {
                self.other.trim()
            } else {
                self.choice.as_str()
            };
            if name.is_empty() {
                return Err("Elegí tu red WiFi de la lista, escribí su nombre o marcá «Sin WiFi».");
            }
            let ssid: Ssid = text(name).ok_or("El nombre de la red es demasiado largo.")?;
            let pass = if self.pass.is_empty() && current.ssid == ssid {
                current.pass.clone()
            } else {
                self.pass.clone()
            };
            if !valid_pass(&pass) {
                return Err("La clave del WiFi tiene que tener entre 8 y 63 caracteres.");
            }
            (ssid, pass)
        };

        let typed = !self.date.trim().is_empty() || !self.time.trim().is_empty();
        let clock = self.clock();
        if !self.auto_time && clock.is_none() {
            return Err(if typed {
                "Esa fecha u hora no existe. Fijate y probá de nuevo."
            } else {
                "Completá la fecha y la hora (o elegí la hora automática)."
            });
        }
        if self.auto_time && ssid.is_empty() && clock.is_none() {
            return Err(
                "Para ponerme en hora solo necesito WiFi. Elegí tu red o poné la hora a mano.",
            );
        }
        let settings = Settings {
            ssid,
            pass,
            auto_time: self.auto_time,
            utc_offset_min: self.zone,
        };
        Ok((settings, clock))
    }
}

// --- Respuestas ----------------------------------------------------------------

/// Atiende un pedido completo (`body_in` es su cuerpo): escribe la cabecera
/// de la respuesta en `head` y el HTML en `body`. Si se eligió algo, lo
/// devuelve para guardarlo y reiniciar.
pub fn respond(
    req: &Request,
    body_in: &[u8],
    cx: &Context,
    head: &mut impl Write,
    body: &mut BufWriter,
) -> Result<Option<Outcome>, fmt::Error> {
    let blank = || Form::from_settings(cx.current);
    let input =
        core::str::from_utf8(body_in).map_err(|_| "El formulario llegó mal, probá de nuevo.");
    let outcome = match route(req) {
        Route::Redirect => {
            head.write_str(captive::REDIRECT)?;
            return Ok(None);
        }
        Route::Page => {
            page(body, cx, &blank(), cx.notice)?;
            None
        }
        Route::Save => match input.and_then(Form::parse) {
            Err(e) => {
                page(body, cx, &blank(), Some(e))?;
                None
            }
            Ok(form) => match form.to_settings(cx.current) {
                Err(e) => {
                    page(body, cx, &form, Some(e))?;
                    None
                }
                Ok((settings, clock)) => Some(Outcome::Saved { settings, clock }),
            },
        },
        route @ (Route::NewMonkey | Route::Factory) => {
            let input = input.unwrap_or("");
            let sure = http::field(input, "seguro") == Some("si");
            let clock = Form::parse(input).ok().and_then(|f| f.clock());
            if !sure {
                page(
                    body,
                    cx,
                    &blank(),
                    Some("Para empezar de nuevo, tildá la casilla de confirmación."),
                )?;
                None
            } else if route == Route::NewMonkey {
                Some(Outcome::NewMonkey { clock })
            } else {
                Some(Outcome::Factory { clock })
            }
        }
    };
    if let Some(o) = &outcome {
        done_page(body, o, cx.version)?;
    }
    captive::head(head, body.len())?;
    Ok(outcome)
}

// --- HTML ----------------------------------------------------------------------

const STYLE: &str = "\
:root{--bg:#fff6ee;--card:#fff;--ink:#4b3426;--mute:#9b8579;--line:#f3e3d7;--pink:#ff8fab;--pinkd:#e9668a;\
--pinkl:#ffe3ec;--mint:#7fd6b2;--mintl:#dff7ec;--lav:#b7a6ff;--lavl:#eee9ff;--ban:#ffd36e;--banl:#fff4d1;--err:#e0566c}\
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%}\
body{margin:0;color:var(--ink);background:var(--bg);\
font:17px/1.5 ui-rounded,'SF Pro Rounded','Nunito','Varela Round','Segoe UI',system-ui,sans-serif}\
main{max-width:30rem;margin:0 auto;padding:1rem 1rem 2.5rem}\
header{text-align:center;padding:.4rem 0 .2rem}header svg{width:7.5rem;height:7.5rem;filter:drop-shadow(0 6px 0 #f1dccd)}\
h1{font-size:1.65rem;margin:.2rem 0 0;letter-spacing:-.01em}.sub{color:var(--mute);margin:.2rem 0 0}\
.card{background:var(--card);border-radius:24px;padding:1.1rem 1.1rem 1.25rem;margin:1.1rem 0;\
box-shadow:0 5px 0 var(--line),0 12px 28px rgba(150,96,60,.10)}\
h2{display:flex;align-items:center;gap:.55rem;font-size:1.2rem;margin:0 0 .25rem}\
h3{font-size:1.05rem;margin:.2rem 0}\
.dot{width:2.2rem;height:2.2rem;border-radius:50%;display:inline-grid;place-items:center;font-size:1.15rem;flex:none}\
.wifi .dot{background:var(--lavl)}.hora .dot{background:var(--mintl)}.again .dot{background:var(--banl)}\
.help{color:var(--mute);font-size:.92rem;margin:.1rem 0 .7rem}\
label{display:block;margin:.55rem 0;font-weight:600}\
input:not([type=radio]):not([type=checkbox]),select{display:block;width:100%;margin-top:.3rem;padding:.75rem .9rem;\
border-radius:14px;border:2px solid var(--line);background:#fffaf6;color:var(--ink);font:inherit;font-weight:400}\
input:focus,select:focus{outline:none;border-color:var(--lav)}\
.opt{display:flex;align-items:center;gap:.7rem;padding:.65rem .8rem;margin:.45rem 0;border:2px solid var(--line);\
border-radius:16px;font-weight:600;cursor:pointer}\
.opt:has(input:checked){border-color:var(--lav);background:var(--lavl)}.hora .opt:has(input:checked){border-color:var(--mint);background:var(--mintl)}\
.opt span{flex:1;min-width:0;overflow-wrap:anywhere}.opt small{display:block;font-weight:400;color:var(--mute);font-size:.86rem}\
.opt i{font-style:normal;color:var(--mute);font-size:.85rem;white-space:nowrap;font-weight:400}\
input[type=radio],input[type=checkbox]{width:1.25rem;height:1.25rem;accent-color:var(--pinkd);flex:none;margin:0}\
.chk{display:flex;gap:.55rem;align-items:center;font-weight:400;color:var(--mute);font-size:.95rem}\
.two{display:flex;gap:.7rem}.two label{flex:1}\
.hora:has(#auto:checked) .mano,.hora:has(#mano:checked) .auto{display:none}\
button{width:100%;padding:1rem;border:0;border-radius:18px;font:inherit;font-weight:800;font-size:1.1rem;cursor:pointer}\
.go{background:linear-gradient(180deg,#ffa3bb,var(--pink));color:#fff;box-shadow:0 5px 0 var(--pinkd);text-shadow:0 1px 0 rgba(0,0,0,.12)}\
.go:active{transform:translateY(3px);box-shadow:0 2px 0 var(--pinkd)}\
.soft{background:var(--banl);color:var(--ink);box-shadow:0 4px 0 #f2dc9c}\
.danger{background:#fff0f2;color:var(--err);box-shadow:0 4px 0 #f6c9d1}\
details summary{list-style:none;cursor:pointer;display:flex;align-items:center;gap:.55rem;font-weight:800;font-size:1.2rem}\
details summary::-webkit-details-marker{display:none}details summary:after{content:'▾';margin-left:auto;color:var(--mute)}\
details[open] summary:after{content:'▴'}\
.reset{border-top:2px dashed var(--line);margin-top:1rem;padding-top:.6rem}.reset p{color:var(--mute);font-size:.93rem;margin:.2rem 0 .4rem}\
.err{background:#fff0f2;border:2px solid #f6c9d1;color:#a8344a;border-radius:16px;padding:.75rem .95rem;font-weight:600}\
.big{font-size:1.1rem}.pill{display:inline-block;background:var(--mintl);border-radius:99px;padding:.15rem .7rem;font-weight:700}\
footer{text-align:center;color:var(--mute);font-size:.85rem;margin-top:1.6rem}";

/// El mono de la casa: pelo marrón, cara clara y gorra.
const MONKEY: &str = "<svg viewBox='0 0 120 120' aria-hidden=true>\
<circle cx=20 cy=64 r=15 fill='#855630'/><circle cx=20 cy=64 r=8.5 fill='#e2b48a'/>\
<circle cx=100 cy=64 r=15 fill='#855630'/><circle cx=100 cy=64 r=8.5 fill='#e2b48a'/>\
<circle cx=60 cy=64 r=40 fill='#855630'/>\
<path d='M60 48c-11-11-32-5-30 13 1 9 6 12 6 19 0 12 11 21 24 21s24-9 24-21c0-7 5-10 6-19 2-18-19-24-30-13z' fill='#f0c9a2'/>\
<path d='M22 46q38-42 76 0z' fill='#7fd6b2'/><path d='M60 44q30 0 46 8q-2 5-8 4q-18-7-38-7z' fill='#5fbf98'/>\
<circle cx=60 cy=21 r=5 fill='#ff8fab'/>\
<ellipse cx=47 cy=66 rx=5.5 ry=6.5 fill='#3b2a20'/><ellipse cx=73 cy=66 rx=5.5 ry=6.5 fill='#3b2a20'/>\
<circle cx=49 cy=63.5 r=2 fill='#fff'/><circle cx=75 cy=63.5 r=2 fill='#fff'/>\
<ellipse cx=38 cy=79 rx=6 ry=3.5 fill='#ff8fab' opacity=.55/><ellipse cx=82 cy=79 rx=6 ry=3.5 fill='#ff8fab' opacity=.55/>\
<path d='M51 83q9 8 18 0' stroke='#3b2a20' stroke-width=3.2 fill=none stroke-linecap=round/></svg>";

/// Llena la fecha y la hora con las del celular (salvo que las toquen) y
/// elige su zona horaria si el mono es nuevo. Sin JavaScript se escriben a mano.
const SCRIPT: &str = "<script>(function(){function p(n){return(n<10?'0':'')+n}\
var q=function(s){return document.querySelectorAll(s)};\
function f(){var d=new Date(),D=d.getFullYear()+'-'+p(d.getMonth()+1)+'-'+p(d.getDate()),H=p(d.getHours())+':'+p(d.getMinutes());\
q('input[name=fecha]').forEach(function(i){if(!i.dataset.t)i.value=D});\
q('input[name=hora]').forEach(function(i){if(!i.dataset.t)i.value=H})}\
q('input[name=fecha],input[name=hora]').forEach(function(i){i.addEventListener('input',function(){i.dataset.t=1})});\
f();setInterval(f,15000);\
var z=document.getElementById('zona'),o=-new Date().getTimezoneOffset();\
if(z&&z.dataset.nuevo)for(var k=0;k<z.options.length;k++)if(+z.options[k].value==o){z.value=o;break}\
var c=document.getElementById('ver');if(c)c.onclick=function(){document.getElementById('clave').type=c.checked?'text':'password'}\
})()</script>";

fn page_start(w: &mut impl Write, title: &str) -> fmt::Result {
    write!(
        w,
        "<!doctype html><html lang=es><head><meta charset=utf-8>\
<meta name=viewport content=\"width=device-width,initial-scale=1\">\
<meta name=theme-color content=\"#fff6ee\"><title>{title}</title><style>{STYLE}</style></head><body><main>\
<header>{MONKEY}"
    )
}

struct Version<'a>(&'a str);

impl fmt::Display for Version<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            Ok(())
        } else {
            write!(f, "v{} ", Esc(self.0))
        }
    }
}

fn page_end(w: &mut impl Write, version: &str) -> fmt::Result {
    write!(
        w,
        "<footer>0xMono {}· hecho a mano con cariño 💛<br>Tu clave del WiFi queda guardada solo en el mono.</footer>\
</main>{SCRIPT}</body></html>",
        Version(version)
    )
}

/// La página. `error` se muestra arriba si el envío anterior no sirvió.
pub fn page(w: &mut impl Write, cx: &Context, form: &Form, error: Option<&str>) -> fmt::Result {
    page_start(w, "Configurá tu 0xMono")?;
    w.write_str(
        "<h1>¡Hola! Soy tu 0xMono</h1>\
<p class=sub>Desde acá me configurás. Cuando guardes, vuelvo a mi casita.</p></header>",
    )?;
    if let Some(e) = error {
        write!(w, "<p class=err>{}</p>", Esc(e))?;
    }

    // --- WiFi ---
    write!(
        w,
        "<form method=post action={SAVE_PATH}><section class='card wifi'>\
<h2><span class=dot>📶</span>Mi WiFi</h2>\
<p class=help>Con WiFi me pongo en hora solo cada vez que me enchufás. Solo veo redes de 2,4 GHz.</p>"
    )?;
    let saved = &cx.current.ssid;
    let mut listed = false;
    let row = |w: &mut dyn Write, ssid: &str, info: &dyn fmt::Display, checked: bool| {
        write!(
            w,
            "<label class=opt><input type=radio name=red value=\"{0}\"{1}><span>{0}</span><i>{2}</i></label>",
            Esc(ssid),
            if checked { " checked" } else { "" },
            info,
        )
    };
    struct Info<'a>(&'a Network, bool);
    impl fmt::Display for Info<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.1 {
                f.write_str("guardada ✓ ")?;
            }
            f.write_str(if self.0.open { "abierta " } else { "🔒 " })?;
            f.write_str(bars(self.0.rssi))
        }
    }
    // La guardada va primero aunque ahora no se vea.
    if !saved.is_empty() && !cx.networks.iter().any(|n| n.ssid == *saved) {
        row(w, saved, &"guardada ✓ (no la veo)", form.choice == *saved)?;
        listed |= form.choice == *saved;
    }
    for n in cx.networks {
        let checked = form.choice == n.ssid;
        listed |= checked;
        row(w, &n.ssid, &Info(n, n.ssid == *saved), checked)?;
    }
    let none = form.no_wifi();
    let other = !listed && !none;
    write!(
        w,
        "<label class=opt><input type=radio name=red value=\"\"{}><span>Otra red<small>Oculta o que no aparece en la lista</small></span></label>\
<input name=otra value=\"{}\" placeholder=\"Nombre exacto de la red\" maxlength=32 autocapitalize=off autocorrect=off spellcheck=false>\
<label class=opt><input type=radio name=red value=\"{NO_WIFI}\"{}><span>Sin WiFi por ahora<small>Me pongo en hora con la que me digas</small></span></label>\
<label>Clave del WiFi<input type=password name=clave id=clave value=\"{}\" placeholder=\"{}\" maxlength=64 \
autocomplete=off autocapitalize=off autocorrect=off spellcheck=false></label>\
<label class=chk><input type=checkbox id=ver>Mostrar la clave</label></section>",
        if other { " checked" } else { "" },
        if other {
            Esc(if form.choice.is_empty() {
                &form.other
            } else {
                &form.choice
            })
        } else {
            Esc("")
        },
        if none { " checked" } else { "" },
        Esc(&form.pass),
        if cx.current.has_wifi() {
            "Vacía = la misma de antes"
        } else {
            "Vacía si la red es abierta"
        },
    )?;

    // --- Hora ---
    let new = !cx.current.has_wifi() && *cx.current == Settings::default();
    write!(
        w,
        "<section class='card hora'><h2><span class=dot>⏰</span>La hora</h2>\
<p class=help>No tengo pila: si me desenchufás, el reloj se frena hasta que vuelvo.</p>\
<label class=opt><input type=radio name=modo value=auto id=auto{}><span>Automática por internet\
<small>Cada vez que me enchufás le pregunto la hora a internet.</small></span></label>\
<div class=auto><label>Zona horaria<select name=zona id=zona{}>",
        if form.auto_time { " checked" } else { "" },
        if new { " data-nuevo=1" } else { "" },
    )?;
    if !ZONES.iter().any(|z| z.offset_min == form.zone) {
        write!(
            w,
            "<option value={0} selected>{1}</option>",
            form.zone,
            Offset(form.zone)
        )?;
    }
    for z in ZONES {
        write!(
            w,
            "<option value={}{}>{} · {}</option>",
            z.offset_min,
            if z.offset_min == form.zone {
                " selected"
            } else {
                ""
            },
            Offset(z.offset_min),
            z.places
        )?;
    }
    w.write_str("</select></label></div>")?;
    let (date, time): (String<16>, String<8>) = match (form.date.is_empty(), cx.now) {
        (true, Some(now)) => {
            let mut d = String::new();
            let mut t = String::new();
            let _ = write!(d, "{}", IsoDate(now));
            let _ = write!(t, "{}", Clock(now));
            (d, t)
        }
        _ => (form.date.clone(), form.time.clone()),
    };
    write!(
        w,
        "<label class=opt><input type=radio name=modo value=mano id=mano{}><span>La pongo yo\
<small>Se toma la del celular; cambiala si querés.</small></span></label>\
<div class='mano two'><label>Fecha<input type=date name=fecha value=\"{}\" placeholder=\"2026-11-11\"></label>\
<label>Hora<input type=time name=hora value=\"{}\" placeholder=\"14:30\"></label></div></section>\
<button class=go>¡Guardar! 🍌</button></form>",
        if form.auto_time { "" } else { " checked" },
        Esc(&date),
        Esc(&time),
    )?;

    // --- Empezar de nuevo ---
    write!(
        w,
        "<details class='card again'><summary><span class=dot>✨</span>Empezar de nuevo</summary>\
<form method=post action={NEW_MONKEY_PATH} class=reset><h3>🐒 Mono nuevo</h3>\
<p>Tu mono se va de viaje y llega otro, con otra personalidad y otra rutina. El WiFi y la hora no se tocan.</p>\
<label class=chk><input type=checkbox name=seguro value=si required>Sí, quiero un mono nuevo</label>\
<input type=hidden name=fecha><input type=hidden name=hora>\
<button class=soft>Traer un mono nuevo</button></form>\
<form method=post action={FACTORY_PATH} class=reset><h3>🧹 Restablecer de fábrica</h3>\
<p>Borra el WiFi, la hora y el mono: quedo como recién salido de la caja.</p>\
<label class=chk><input type=checkbox name=seguro value=si required>Sí, borrar todo</label>\
<input type=hidden name=fecha><input type=hidden name=hora>\
<button class=danger>Borrar todo</button></form></details>"
    )?;
    page_end(w, cx.version)
}

/// La página de "¡listo!" después de elegir algo.
pub fn done_page(w: &mut impl Write, o: &Outcome, version: &str) -> fmt::Result {
    page_start(w, "¡Listo!")?;
    match o {
        Outcome::Saved { settings, clock } => {
            w.write_str(
                "<h1>¡Listo! 🎉</h1><p class=sub>Guardé todo.</p></header><section class=card>",
            )?;
            if settings.wants_sync() {
                write!(
                    w,
                    "<p class=big>Me reinicio, me conecto a <b>{}</b> y le pregunto la hora a internet.</p>",
                    Esc(&settings.ssid)
                )?;
            } else if let Some(t) = clock {
                write!(
                    w,
                    "<p class=big>Me reinicio y vuelvo a mi casita. Para mí son las <span class=pill>{}</span>.</p>",
                    Clock(*t)
                )?;
            } else {
                w.write_str("<p class=big>Me reinicio y vuelvo a mi casita.</p>")?;
            }
        }
        Outcome::NewMonkey { .. } => {
            w.write_str(
                "<h1>¡Llega un mono nuevo! 🐒</h1><p class=sub>El anterior se fue de viaje, tranqui.</p></header>\
<section class=card><p class=big>En un ratito aparece en la casa, con su propia personalidad. ¡Saludalo!</p>",
            )?;
        }
        Outcome::Factory { .. } => {
            w.write_str(
                "<h1>Todo borrado ✨</h1><p class=sub>Como recién salido de la caja.</p></header>\
<section class=card><p class=big>Me reinicio con un mono nuevo. Cuando quieras, volvé a configurarme desde el menú.</p>",
            )?;
        }
    }
    w.write_str(
        "<p class=help>Ya podés cerrar esta página: tu celular vuelve solo a su WiFi.</p></section>",
    )?;
    page_end(w, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use captive::add_network;
    use heapless::Vec;

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

    fn networks() -> Vec<Network, 16> {
        let mut v = Vec::new();
        add_network(&mut v, "WiFi de casa", -48, false);
        add_network(&mut v, "Café Libre", -66, true);
        v
    }

    fn post(path: &str, form: &str, current: &Settings) -> (Option<Outcome>, std::string::String) {
        let raw = std::format!(
            "POST {path} HTTP/1.1\r\nHost: 192.168.4.1\r\nContent-Length: {}\r\n\r\n{form}",
            form.len()
        );
        let req = http::parse(raw.as_bytes()).unwrap();
        let body_in = &raw.as_bytes()[req.body_start..];
        let nets = networks();
        let cx = Context {
            networks: &nets,
            current,
            notice: None,
            now: None,
            version: "0.1.0",
        };
        let mut head_buf = [0u8; 256];
        let mut body_buf = std::vec![0u8; MAX_PAGE];
        let mut head = BufWriter::new(&mut head_buf);
        let mut body = BufWriter::new(&mut body_buf);
        let out = respond(&req, body_in, &cx, &mut head, &mut body).unwrap();
        let head = std::string::String::from_utf8(head.as_bytes().to_vec()).unwrap();
        let body = std::string::String::from_utf8(body.as_bytes().to_vec()).unwrap();
        assert!(head.contains(&std::format!("Content-Length: {}", body.len())));
        (out, body)
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
        assert_eq!(route(&req(Method::Get, AP_HOST, "/")), Route::Page);
        assert_eq!(route(&req(Method::Get, "", "/")), Route::Page);
        assert_eq!(route(&req(Method::Post, AP_HOST, "/guardar")), Route::Save);
        assert_eq!(
            route(&req(Method::Post, AP_HOST, "/mono-nuevo")),
            Route::NewMonkey
        );
        assert_eq!(
            route(&req(Method::Post, AP_HOST, "/de-fabrica")),
            Route::Factory
        );
        assert_eq!(
            route(&req(Method::Get, AP_HOST, "/de-fabrica")),
            Route::Redirect
        );
    }

    #[test]
    fn wifi_with_automatic_time_syncs_after_saving() {
        let (out, body) = post(
            SAVE_PATH,
            "red=WiFi+de+casa&otra=&clave=12345678&modo=auto&zona=-180&fecha=2026-11-11&hora=14%3A32",
            &Settings::default(),
        );
        let Some(Outcome::Saved { settings, clock }) = out.clone() else {
            panic!("{out:?}")
        };
        assert_eq!(settings.ssid, "WiFi de casa");
        assert_eq!(settings.pass, "12345678");
        assert!(settings.auto_time);
        assert_eq!(settings.utc_offset_min, -180);
        assert_eq!(
            clock,
            Some(date::local_time("2026-11-11", "14:32").unwrap())
        );
        assert!(body.contains("¡Listo!") && body.contains("WiFi de casa"));
        assert_eq!(
            out.unwrap().next(),
            Next::Sync(SyncStart {
                manual: true,
                new_monkey: false,
                fallback: clock
            })
        );
    }

    #[test]
    fn manual_time_needs_a_real_date() {
        let s = Settings::default();
        let (out, body) = post(
            SAVE_PATH,
            "red=*sin-wifi*&modo=mano&zona=-180&fecha=&hora=",
            &s,
        );
        assert!(out.is_none());
        assert!(body.contains("Completá la fecha"));
        let (out, body) = post(
            SAVE_PATH,
            "red=*sin-wifi*&modo=mano&zona=-180&fecha=2026-02-30&hora=10%3A00",
            &s,
        );
        assert!(out.is_none());
        assert!(body.contains("no existe"));
        let (out, _) = post(
            SAVE_PATH,
            "red=*sin-wifi*&modo=mano&zona=-180&fecha=2026-11-14&hora=10%3A00",
            &s,
        );
        let out = out.unwrap();
        let Outcome::Saved { settings, clock } = &out else {
            panic!()
        };
        assert!(!settings.has_wifi() && !settings.auto_time);
        let t = clock.unwrap();
        assert_eq!(t.weekday(), companion::clock::Weekday::Sat);
        assert_eq!(
            out.next(),
            Next::House(HouseStart {
                clock: Some(t),
                new_monkey: false,
                notice: Some(Notice::Saved)
            })
        );
    }

    #[test]
    fn friendly_errors_keep_what_was_typed() {
        let s = Settings::default();
        let (out, body) = post(SAVE_PATH, "red=&otra=&clave=&modo=auto&zona=-180", &s);
        assert!(out.is_none() && body.contains("Elegí tu red"));
        let (out, body) = post(
            SAVE_PATH,
            "red=&otra=Mi+%3Cred%3E&clave=corta&modo=auto",
            &s,
        );
        assert!(out.is_none() && body.contains("entre 8 y 63"));
        assert!(
            body.contains("value=\"Mi &lt;red&gt;\""),
            "conserva la red escrita"
        );
        let (out, body) = post(SAVE_PATH, "red=*sin-wifi*&modo=auto&zona=-180", &s);
        assert!(out.is_none() && body.contains("necesito WiFi"));
    }

    #[test]
    fn empty_password_keeps_the_saved_one_for_the_same_network() {
        let current = Settings {
            ssid: text("WiFi de casa").unwrap(),
            pass: text("clave-vieja").unwrap(),
            ..Settings::default()
        };
        let (out, _) = post(
            SAVE_PATH,
            "red=WiFi+de+casa&clave=&modo=auto&zona=-300",
            &current,
        );
        let Some(Outcome::Saved { settings, .. }) = out else {
            panic!()
        };
        assert_eq!(settings.pass, "clave-vieja");
        assert_eq!(settings.utc_offset_min, -300);
        let (out, _) = post(SAVE_PATH, "red=Caf%C3%A9+Libre&clave=&modo=auto", &current);
        let Some(Outcome::Saved { settings, .. }) = out else {
            panic!()
        };
        assert_eq!(
            (settings.ssid.as_str(), settings.pass.as_str()),
            ("Café Libre", "")
        );
    }

    #[test]
    fn starting_over_needs_the_checkbox() {
        let s = Settings::default();
        let (out, body) = post(NEW_MONKEY_PATH, "fecha=&hora=", &s);
        assert!(out.is_none() && body.contains("tildá la casilla"));
        let (out, body) = post(
            NEW_MONKEY_PATH,
            "seguro=si&fecha=2026-11-11&hora=09%3A15",
            &s,
        );
        let out = out.unwrap();
        assert!(body.contains("mono nuevo"));
        let Next::House(h) = out.next() else { panic!() };
        assert!(h.new_monkey);
        assert_eq!(h.clock.map(|t| t.hour()), Some(9));
        assert_eq!(h.notice, Some(Notice::NewMonkey));
        let (out, body) = post(FACTORY_PATH, "seguro=si", &s);
        assert_eq!(out, Some(Outcome::Factory { clock: None }));
        assert!(body.contains("Todo borrado"));
    }

    #[test]
    fn page_escapes_fits_and_needs_nothing_from_internet() {
        let mut nets: Vec<Network, 16> = Vec::new();
        for i in 0..16 {
            let mut name: String<32> = String::new();
            let _ = write!(name, "Red \"<{i}>\" con nombre largo xx");
            add_network(&mut nets, &name, -40 - i as i8 * 3, i % 3 == 0);
        }
        let current = Settings {
            ssid: text("<script>").unwrap(),
            utc_offset_min: 345,
            ..Settings::default()
        };
        let cx = Context {
            networks: &nets,
            current: &current,
            notice: Some("a < b"),
            now: Some(LocalTime::new(20_768, 14, 32)),
            version: "0.1.0",
        };
        let mut buf = std::vec![0u8; MAX_PAGE];
        let mut w = BufWriter::new(&mut buf);
        page(&mut w, &cx, &Form::from_settings(&current), cx.notice).unwrap();
        let html = core::str::from_utf8(w.as_bytes()).unwrap();
        assert!(!html.contains("<script>\""));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("a &lt; b"));
        assert!(
            html.contains("UTC+5:45"),
            "zona guardada que no está en la lista"
        );
        assert!(html.contains("value=\"2026-11-11\"") && html.contains("value=\"14:32\""));
        for external in ["http://", "https://", "//fonts", "@import", "src="] {
            assert!(!html.contains(external), "{external}");
        }
        assert!(w.len() < MAX_PAGE - 1024, "{} bytes", w.len());
    }
}
