//! Las frases del mono: un dev obsesionado con las cripto y el software,
//! gamer, fan de las bananas y de los fines de semana tranquilos.
//!
//! Cada [`Rule`] dice cuándo vale (actividad, hora, tipo de día, humor,
//! mercado, gustos) y tiene huecos entre llaves que se llenan al armarla
//! (ver [`pool`], [`did`], [`next`] y `talk::fill`). [`SAY`]: cuando le
//! hablás; [`THINK`]: lo que piensa en voz alta.
//!
//! Para agregar frases alcanza con sumar reglas acá; los tests de `talk`
//! controlan que entren en el globo y que no queden huecos sin llenar.

use crate::activity::Activity as A;
use crate::clock::Weekday;
use crate::companion::Greeting;
use crate::personality::Personality;
use crate::routine::{DayKind, Mood};

pub const SAY: u8 = 1;
pub const THINK: u8 = 2;
const BOTH: u8 = SAY | THINK;

/// Condición sobre el mercado.
pub const ANY: i8 = 0;
pub const UP: i8 = 1;
pub const DOWN: i8 = 2;
pub const FLAT: i8 = 3;
pub const MOON: i8 = 4;
pub const DUMP: i8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taste {
    Sporty,
    Bookish,
    Geeky,
    GreenThumb,
    Sociable,
    Musical,
    Gamer,
    Foodie,
    Chill,
}

impl Taste {
    pub fn of(self, p: &Personality) -> u8 {
        match self {
            Taste::Sporty => p.sporty,
            Taste::Bookish => p.bookish,
            Taste::Geeky => p.geeky,
            Taste::GreenThumb => p.green_thumb,
            Taste::Sociable => p.sociable,
            Taste::Musical => p.musical,
            Taste::Gamer => p.gamer,
            Taste::Foodie => p.foodie,
            Taste::Chill => p.chill,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub text: &'static str,
    /// Actividades en las que vale (un bit cada una); 0 = cualquiera.
    pub acts: u32,
    /// [`SAY`], [`THINK`] o los dos.
    pub kind: u8,
    /// Minutos del día en que vale (`from..to`, puede cruzar la medianoche).
    pub from: u16,
    pub to: u16,
    /// Días: 1 semana, 2 sábado, 4 domingo; 0 = todos.
    pub days: u8,
    /// Humores del día (un bit por `Mood`); 0 = cualquiera.
    pub moods: u8,
    pub market: i8,
    pub taste: Option<(Taste, u8)>,
    pub weight: u8,
}

const fn b(a: A) -> u32 {
    1 << a as u32
}

const fn rule(acts: u32, kind: u8, text: &'static str) -> Rule {
    Rule {
        text,
        acts,
        kind,
        from: 0,
        to: 1440,
        days: 0,
        moods: 0,
        market: ANY,
        taste: None,
        weight: 10,
    }
}

/// Lo dice cuando le hablás.
const fn s(acts: u32, text: &'static str) -> Rule {
    rule(acts, SAY, text)
}

/// Lo piensa en voz alta.
const fn t(acts: u32, text: &'static str) -> Rule {
    rule(acts, THINK, text)
}

/// Las dos cosas.
const fn st(acts: u32, text: &'static str) -> Rule {
    rule(acts, BOTH, text)
}

impl Rule {
    /// Solo entre las `from` y las `to` (horas; puede cruzar la medianoche).
    const fn h(mut self, from: u16, to: u16) -> Self {
        self.from = from * 60;
        self.to = to * 60;
        self
    }
    const fn wd(mut self) -> Self {
        self.days = 1;
        self
    }
    const fn we(mut self) -> Self {
        self.days = 2 | 4;
        self
    }
    const fn sat(mut self) -> Self {
        self.days = 2;
        self
    }
    const fn sun(mut self) -> Self {
        self.days = 4;
        self
    }
    const fn mood(mut self, m: Mood) -> Self {
        self.moods |= 1 << m as u32;
        self
    }
    const fn up(mut self) -> Self {
        self.market = UP;
        self
    }
    const fn down(mut self) -> Self {
        self.market = DOWN;
        self
    }
    const fn flat(mut self) -> Self {
        self.market = FLAT;
        self
    }
    const fn moon(mut self) -> Self {
        self.market = MOON;
        self
    }
    const fn dump(mut self) -> Self {
        self.market = DUMP;
        self
    }
    const fn likes(mut self, taste: Taste, min: u8) -> Self {
        self.taste = Some((taste, min));
        self
    }
    const fn w(mut self, weight: u8) -> Self {
        self.weight = weight;
        self
    }
}

/// Plantilla para los saludos (sus textos salen de [`greetings`]).
pub const GREETING_RULE: Rule = rule(0, SAY, "");

const ANY_ACT: u32 = 0;
const CODE: u32 = b(A::Code);
const MEET: u32 = b(A::Meeting);
const TRADE: u32 = b(A::Trading);
const WORK: u32 = CODE | MEET | TRADE;
const READ: u32 = b(A::Read);
const EAT: u32 = b(A::Eat) | b(A::Snack);
const COOK: u32 = b(A::Cook);
const MATE: u32 = b(A::Mate);
const GUITAR: u32 = b(A::Guitar);
const DANCE: u32 = b(A::Dance);
const GAMES: u32 = b(A::Games);
const MOVIE: u32 = b(A::Movie);
const SPORT: u32 = b(A::Exercise) | b(A::Stretch);
const GARDEN: u32 = b(A::Garden);
const BANANAS: u32 = b(A::Bananas);
const CLEAN: u32 = b(A::Clean);
const PHONE: u32 = b(A::Phone);
const STARS: u32 = b(A::Stargaze);
const DREAM: u32 = b(A::Daydream);
const WANDER: u32 = b(A::Wander);
const WAKE: u32 = b(A::WakeUp);
const GROGGY: u32 = b(A::Groggy);
const SLEEP: u32 = b(A::Sleep) | b(A::Nap);
const NAP: u32 = b(A::Nap);
const AWAY: u32 = b(A::Away);
/// Despierto y en casa (para las frases generales).
pub const AWAKE: u32 = !(SLEEP | AWAY | GROGGY) & ((1 << 26) - 1);
const RELAX: u32 = MATE | DREAM | STARS | WANDER;
const SEATED_FUN: u32 = GAMES | MOVIE | GUITAR;

use Mood::{Creative, Energetic, Focused, Lazy, Outdoorsy, Social};

pub static RULES: &[Rule] = &[
    // ------------------------------------------------------------------
    // Programando (trabajo y hobby).
    // ------------------------------------------------------------------
    s(CODE, "Shh, estoy en la zona. Diez minutos y soy tuyo."),
    s(CODE, "Peleándome con {bug}. Ya lo tengo acorralado."),
    s(CODE, "¿Viste que {tool} me odia? Bueno, yo también."),
    s(CODE, "Estoy armando {feature}. Va a quedar hermoso."),
    s(CODE, "Escribiendo {lang}. Mis dedos van solos."),
    s(CODE, "Compiló al primer intento. Desconfío."),
    s(CODE, "Hoy deployo {feature} a {chain}. Cruzá los dedos."),
    s(CODE, "Tengo {n} pestañas de Stack Overflow abiertas."),
    s(CODE, "Refactorizando. O sea, rompiendo cosas con estilo."),
    s(CODE, "Los tests pasan. Todos. Algo anda mal."),
    s(CODE, "¿Sabías que los bugs se reproducen de noche?"),
    s(CODE, "Escribo código como como bananas: sin parar."),
    s(CODE, "Si funciona, no lo toco. Regla de oro."),
    s(CODE, "Un smart contract más y me compro una isla."),
    s(CODE, "Estoy auditando un contrato. Huele a rug pull."),
    s(CODE, "Commit, push, rezar. El ciclo de la vida."),
    s(CODE, "Me falta un punto y coma. Lo siento en el alma."),
    s(CODE, "Mi código no tiene bugs, tiene sorpresas."),
    s(CODE, "Estoy optimizando el gas. Cada gwei cuenta."),
    s(CODE, "Mergeé a main un viernes. Vivo al límite."),
    s(CODE, "Ya casi termino {feature}. Ya casi. Casi."),
    s(CODE, "¿Me ves? Soy un 10x developer. Bueno, 2x."),
    s(CODE, "{lang} es hermoso cuando te deja compilar."),
    s(CODE, "Encontré {bug} en producción. Respiro hondo."),
    s(CODE, "Programar con bananas cerca rinde el doble."),
    s(CODE, "Estoy leyendo la doc. Sí, la doc. Increíble."),
    s(CODE, "Este código lo escribí yo y no lo entiendo."),
    s(CODE, "Necesito un café. O un mate. O tres bananas."),
    s(CODE, "Hoy le gané a {tool}. Día histórico."),
    s(CODE, "Escribí {big} líneas. Borré {n}0. Progreso."),
    s(CODE, "Laburando en un DEX descentralizado de bananas."),
    s(CODE, "Mi nodo de {chain} anda joya, por ahora."),
    s(CODE, "Probando en testnet antes de romper mainnet."),
    s(CODE, "Los commits de hoy van a ser legendarios."),
    s(CODE, "Fijate: verde, verde, verde. El CI me ama."),
    s(CODE, "Rojo. El CI está rojo. No me mires."),
    s(CODE, "Estoy en modo hacker. Bah, en modo mono.").likes(Taste::Geeky, 50),
    s(CODE, "Un ratito más de código y salgo, lo prometo.").h(18, 23),
    s(CODE, "Programar de noche es otra cosa. Silencio total.").h(21, 4),
    s(CODE, "Primer commit del día. Que la fuerza me acompañe.").h(7, 11),
    s(CODE, "Fin de semana y programo igual. Es amor.").we(),
    s(CODE, "Proyecto personal de finde: un juego con NFTs.").we(),
    s(CODE, "Lunes. Arranco tranquilo con {feature}.").wd().w(4),
    s(CODE, "Viernes, nada de deploys. Bueno, uno chiquito.")
        .wd()
        .w(4),
    t(CODE, "Mmm... ¿y si lo reescribo en {lang}?"),
    t(CODE, "¿Quién escribió esto? Ah, fui yo."),
    t(CODE, "Dale, compilá. Dale. Dale..."),
    t(CODE, "Un bug menos. Quedan {n}."),
    t(CODE, "Esto con un buen test no pasaba."),
    t(CODE, "Me encanta cuando todo encaja."),
    t(CODE, "¿{bug}? ¿Otra vez vos?"),
    t(CODE, "Nota mental: documentar esto. Nunca."),
    t(CODE, "Ctrl+S, Ctrl+S, Ctrl+S, por las dudas."),
    t(CODE, "{tool}, sos mi amigo. A veces."),
    t(CODE, "¡Funciona! No sé por qué, pero funciona."),
    t(CODE, "No funciona. Tampoco sé por qué."),
    t(CODE, "Este {feature} va a romperla."),
    t(CODE, "Gas fee de {n} gwei... aceptable."),
    t(CODE, "Si lo deployo ahora, ¿qué puede salir mal?"),
    t(CODE, "Necesito un nombre para esta variable... 'cosa2'."),
    t(CODE, "Ahh, era un off-by-one. Clásico."),
    t(CODE, "Ese commit merece una banana de premio."),
    t(CODE, "Ya está, ya está... no, no está."),
    t(CODE, "El borrow checker tiene razón. Odio que tenga razón.").likes(Taste::Geeky, 40),
    // ------------------------------------------------------------------
    // Reuniones.
    // ------------------------------------------------------------------
    s(MEET, "Estoy en una call. Hacé de cuenta que no estás."),
    s(MEET, "Reunión de planning. Planeamos planear."),
    s(MEET, "Shh, me están por preguntar algo. ¿Qué dijeron?"),
    s(MEET, "Esta reunión podría haber sido un mail."),
    s(MEET, "Tengo la cámara prendida, saludá. ¡Hola!"),
    s(MEET, "Daily. Hoy voy a decir: 'sigo con {feature}'."),
    s(MEET, "Me dijeron que estoy muteado hace 5 minutos."),
    s(MEET, "Presentando {feature} al equipo. Nervios."),
    s(MEET, "Call con inversores. Pongo cara de serio."),
    s(MEET, "Están discutiendo tabs o espacios. Otra vez."),
    s(MEET, "La reunión era de 15 min. Vamos {n}0."),
    s(MEET, "Hablando de la roadmap de {chain}. Interesante."),
    t(MEET, "¿Estoy muteado? Creo que no..."),
    t(MEET, "Asentir, asentir, asentir..."),
    t(MEET, "¿Me toca hablar? Me toca hablar."),
    t(MEET, "Que no se note que estoy pensando en bananas."),
    t(MEET, "Buen punto. No sé cuál, pero buen punto."),
    t(MEET, "¿Cuánto falta? Mi panza hace ruido."),
    t(MEET, "Esto se resolvía con un PR de dos líneas."),
    // ------------------------------------------------------------------
    // Mirando el mercado.
    // ------------------------------------------------------------------
    s(TRADE, "¡{coin} a la luna! +{pct}% ¡Te lo dije!").moon(),
    s(TRADE, "¡Estoy rico! Bueno, un poquito. Por ahora.").moon(),
    s(TRADE, "WAGMI. Lo sabía. Siempre lo supe.").moon(),
    s(TRADE, "¡{coin} explotó! Diamond hands, bebé.").moon(),
    s(TRADE, "Mirá ese gráfico. Es una banana para arriba.").moon(),
    s(TRADE, "Si sigue así me compro un bananero de oro.").moon(),
    s(TRADE, "¡Velas verdes por todos lados! ¡Fiesta!").moon(),
    s(TRADE, "{coin} sube {pct}%. Tranqui, nada de FOMO.").up(),
    s(TRADE, "Va subiendo... no lo mires que se asusta.").up(),
    s(TRADE, "Lindo día para estar en verde.").up(),
    s(TRADE, "Compré el dip ayer. Soy un genio, ¿no?").up(),
    s(TRADE, "Verde suave. Así me gusta, sin sustos.").up(),
    s(TRADE, "HODL. Esa es toda mi estrategia.").up(),
    s(TRADE, "{coin} un {pct}% arriba. Me sirvo un mate.").up(),
    s(TRADE, "Bull market, baby. Bah, bull mañana.").up(),
    s(TRADE, "{coin} bajó {pct}%. No vendo. No vendo.").down(),
    s(TRADE, "Esto es una corrección sana. Muy sana.").down(),
    s(TRADE, "Rojo. Todo rojo. Necesito una banana.").down(),
    s(TRADE, "Compro el dip. ¿Era el dip? Era el dip.").down(),
    s(TRADE, "No importa, es a largo plazo. Muy largo.").down(),
    s(TRADE, "Si no vendés, no perdés. Eso dicen.").down(),
    s(TRADE, "Paper hands no. Diamond hands. Llorando.").down(),
    s(TRADE, "{coin} se desplomó {pct}%. Estoy bien. Bien.").dump(),
    s(TRADE, "Esto es un rug pull o un mal sueño.").dump(),
    s(TRADE, "Me quedé en cero... y en bananas, por suerte.").dump(),
    s(TRADE, "Cierro la app. Abro la app. Cierro la app.").dump(),
    s(TRADE, "Mi portfolio hizo un clavado olímpico.").dump(),
    s(TRADE, "Lateral. Aburrido. Perfecto para acumular.").flat(),
    s(TRADE, "{coin} no se mueve. Yo tampoco.").flat(),
    s(TRADE, "Mercado dormido. Calma antes de la tormenta.").flat(),
    s(TRADE, "Analizando el gráfico. Veo un mono. Ah, soy yo.").flat(),
    s(TRADE, "Ni sube ni baja. Hora de investigar proyectos.").flat(),
    s(TRADE, "Mirando el whitepaper de un token nuevo."),
    s(TRADE, "No es consejo financiero, pero... bananas."),
    s(TRADE, "Estoy leyendo los charts como un oráculo."),
    s(TRADE, "Mi estrategia: DCA y paciencia de mono."),
    t(TRADE, "Vamos, vamos, verde...").up(),
    t(TRADE, "¡Eso! ¡A la luna!").moon(),
    t(TRADE, "No mires el precio. No mires el precio.").down(),
    t(TRADE, "Esto ya lo viví. Sobreviví.").dump(),
    t(TRADE, "Soporte en... ningún lado. Genial.").dump(),
    t(TRADE, "Mmm, esta vela parece una banana."),
    t(TRADE, "¿Compro? ¿Vendo? ¿Como una banana?"),
    t(TRADE, "Si hubiera comprado en 2015..."),
    t(TRADE, "Hora de HODLear con todas las fuerzas."),
    t(TRADE, "Ese volumen es raro... muy raro."),
    t(TRADE, "Lateral. Bostezo.").flat(),
    // ------------------------------------------------------------------
    // Leyendo.
    // ------------------------------------------------------------------
    s(READ, "Leyendo el whitepaper de Bitcoin. Otra vez."),
    s(READ, "Este libro de algoritmos es mejor que una novela."),
    s(READ, "Novela de ciencia ficción: monos en el espacio."),
    s(READ, "Estoy en el capítulo {n}. No me spoilees."),
    s(READ, "Leer en el sillón con mate. El paraíso."),
    s(READ, "Libro de cocina con bananas. Tomo nota."),
    s(READ, "Aprendiendo {lang} de un libro. Old school."),
    s(READ, "Un libro sobre la historia de internet. Atrapante."),
    s(READ, "Novela policial. Sospecho del mayordomo."),
    s(READ, "Estoy leyendo 'Clean Code'. Me siento juzgado."),
    s(READ, "Este autor escribe como yo programo: con pasión."),
    s(READ, "Un libro de jardinería. Para {plant}.").likes(Taste::GreenThumb, 40),
    s(READ, "Me quedo hasta terminar el capítulo.").h(21, 2),
    t(READ, "Uh, qué giro..."),
    t(READ, "Una página más. Y otra. Y otra..."),
    t(READ, "Esto lo tengo que subrayar."),
    t(READ, "Ja, buenísimo este párrafo."),
    t(READ, "¿Y si el villano tenía razón?"),
    t(READ, "Me acaba de dar una idea para {feature}."),
    // ------------------------------------------------------------------
    // Comiendo y merendando.
    // ------------------------------------------------------------------
    s(EAT, "¡Mmm! {food}. Lo mejor del día."),
    s(EAT, "Comiendo {food}. ¿Querés? Bueno, no te doy."),
    s(EAT, "Banana número {n} del día. Llevo la cuenta."),
    s(EAT, "Con la panza llena se programa mejor."),
    s(EAT, "Esta banana está en su punto justo."),
    s(EAT, "Pausa para comer. El código puede esperar."),
    s(EAT, "Comer bananas es mi cardio."),
    s(EAT, "¿Sabías que tengo {n} recetas con banana?"),
    s(EAT, "Mmm, {food}... perdón, tengo la boca llena."),
    s(EAT, "Desayuno de campeones: bananas y mate.").h(6, 11),
    s(EAT, "Almuerzo rapidito y vuelvo al laburo.")
        .h(11, 15)
        .wd(),
    s(EAT, "Almuerzo largo de finde. Sin apuro.").h(11, 16).we(),
    s(EAT, "Cena liviana: {food}. Bah, liviana...").h(19, 24),
    s(EAT, "Merienda: {food} con mate. Clásico.").h(15, 19),
    s(EAT, "La mesa es mi lugar favorito de la casa."),
    s(EAT, "Comí tanto que entro en modo siesta."),
    s(EAT, "Esta la coseché yo. Sabe a victoria."),
    t(EAT, "Ñam."),
    t(EAT, "Una más y paro. Bueno, dos."),
    t(EAT, "Esto está buenísimo."),
    t(EAT, "¿Quedan bananas en la frutera?"),
    t(EAT, "Comida rica, código rico."),
    // ------------------------------------------------------------------
    // Cocinando.
    // ------------------------------------------------------------------
    s(COOK, "Cocinando {dish}. Olé el aroma."),
    s(COOK, "Chef mono en acción. No toques nada."),
    s(COOK, "Le pongo banana a todo. Es mi firma."),
    s(COOK, "Receta secreta de la abuela mona."),
    s(COOK, "Revolver, probar, revolver. Así se cocina."),
    s(COOK, "Esto va a quedar mejor que mi código."),
    s(COOK, "Cocinar es como programar, pero se come."),
    s(COOK, "{dish} para {n}. Sí, como mucho."),
    s(COOK, "Si se quema, lo llamo 'ahumado'."),
    s(COOK, "El secreto es el amor. Y la sal."),
    s(COOK, "Domingo de cocina grande. Sin apuro.").sun(),
    s(COOK, "Me encanta cocinar para alguien.").likes(Taste::Foodie, 50),
    t(COOK, "Una pizca más... ahí."),
    t(COOK, "¿Le puse sal? Creo que sí. ¿Sí?"),
    t(COOK, "Huele increíble."),
    t(COOK, "Que no se pegue, que no se pegue..."),
    t(COOK, "Pruebo... ¡Uf, quema!"),
    t(COOK, "Esto lleva un toque de banana. Confiá."),
    // ------------------------------------------------------------------
    // Mate.
    // ------------------------------------------------------------------
    s(MATE, "Tomando unos mates. ¿Te cebo uno?"),
    s(MATE, "El mate es el mejor debugger."),
    s(MATE, "Mate y silencio. Lo que necesitaba."),
    s(MATE, "Este mate está espectacular. Agua justa."),
    s(MATE, "Un mate y arranco. Bueno, dos."),
    s(MATE, "Mate dulce o amargo, no importa: mate."),
    s(MATE, "Ronda de mate con el sol entrando. Hermoso.").h(7, 18),
    s(MATE, "Mate de finde. No hay apuro para nada.").we(),
    s(MATE, "Pausa de mate entre reunión y reunión.").wd(),
    s(MATE, "El termo nunca me falla. Ojalá el CI igual."),
    s(MATE, "Mirá, el mate se lavó. Hora de cambiar yerba."),
    t(MATE, "Ahh... qué rico."),
    t(MATE, "Sorbito... sorbito..."),
    t(MATE, "El mate arregla todo."),
    t(MATE, "¿Le pongo un poquito más de agua?"),
    t(MATE, "Un mate más y me pongo a programar. Mentira."),
    // ------------------------------------------------------------------
    // Guitarra.
    // ------------------------------------------------------------------
    s(GUITAR, "Practicando {song}. ¿Cómo sueno?"),
    s(GUITAR, "Sé tres acordes. Con eso alcanza."),
    s(GUITAR, "Compuse una canción sobre {coin}."),
    s(GUITAR, "La guitarra me relaja más que el mate."),
    s(GUITAR, "Esta va dedicada a mis bananas."),
    s(GUITAR, "¿Me pedís un tema? Sé {n}. Más o menos."),
    s(GUITAR, "Desafinada, como mi código. Pero suena."),
    s(GUITAR, "Algún día toco en un estadio. O en la plaza."),
    s(GUITAR, "Un temita antes de dormir.").h(21, 2),
    t(GUITAR, "La, mi, re... ¿re o mi?"),
    t(GUITAR, "Esta parte me sale. Esta no."),
    t(GUITAR, "Sonó lindo eso."),
    t(GUITAR, "Tengo que cambiar las cuerdas."),
    // ------------------------------------------------------------------
    // Bailando.
    // ------------------------------------------------------------------
    s(DANCE, "¡Mirá estos pasos! Puro talento."),
    s(DANCE, "Bailo como si nadie mirara. Pero mirás vos."),
    s(DANCE, "Baile de la victoria: {coin} subió.").up(),
    s(DANCE, "Bailar es mi forma de festejar un deploy."),
    s(DANCE, "Este paso se llama 'la banana loca'."),
    s(DANCE, "¡Sumate! Uno, dos, uno, dos."),
    s(DANCE, "Sábado de baile. ¡Música!").sat(),
    s(DANCE, "Quemo las bananas del almuerzo."),
    t(DANCE, "¡Eso! ¡Eso!"),
    t(DANCE, "Paso a la izquierda, paso a la derecha..."),
    t(DANCE, "Soy una estrella. Una estrella mona."),
    t(DANCE, "¿Me salió? ¡Me salió!"),
    // ------------------------------------------------------------------
    // Consola.
    // ------------------------------------------------------------------
    s(GAMES, "Jugando {game}. No me hables, boss final."),
    s(GAMES, "Una partida más y lo dejo. Mentira."),
    s(GAMES, "Llevo {n} horas en {game}. Bueno, {big}."),
    s(GAMES, "Speedrun de {game}. Récord personal cerca."),
    s(GAMES, "Me mataron. No fue mi culpa, fue el lag."),
    s(GAMES, "¿Jugamos? Te presto el otro joystick."),
    s(GAMES, "Este nivel me tiene harto. Una vez más."),
    s(GAMES, "Encontré un easter egg. Soy un genio."),
    s(GAMES, "Los juegos retro son los mejores. Punto."),
    s(GAMES, "Jugando a {game} y comiendo bananas. Plenitud."),
    s(GAMES, "Finde de consola. El mejor plan.").we(),
    s(GAMES, "Me hice un personaje mono. Obvio.").likes(Taste::Gamer, 40),
    s(GAMES, "Si gano esta, me voy a dormir. Promesa.").h(22, 3),
    t(GAMES, "¡No, no, no! Casi..."),
    t(GAMES, "Esquivo, salto, ¡tomá!"),
    t(GAMES, "Este boss tiene un patrón... lo veo."),
    t(GAMES, "Guardo partida. Por las dudas."),
    t(GAMES, "¡Logro desbloqueado!"),
    t(GAMES, "Una más. Solo una más."),
    // ------------------------------------------------------------------
    // Película.
    // ------------------------------------------------------------------
    s(MOVIE, "Viendo {movie}. ¿Traés pochoclos?"),
    s(MOVIE, "Shh, que viene la mejor parte."),
    s(MOVIE, "Ya adiviné el final. No me preguntes cómo."),
    s(MOVIE, "Película y sillón. Plan perfecto."),
    s(MOVIE, "Ese hacker de la peli escribe HTML. Jaja."),
    s(MOVIE, "Si hay monos en la peli, es buena."),
    s(MOVIE, "Noche de peli. Nada de laburo.").h(19, 3),
    t(MOVIE, "Uh, no abras esa puerta..."),
    t(MOVIE, "Jaja, buenísimo."),
    t(MOVIE, "Ese actor es igual a mi primo."),
    t(MOVIE, "¿Pausa para buscar una banana?"),
    // ------------------------------------------------------------------
    // Ejercicio y elongación.
    // ------------------------------------------------------------------
    s(SPORT, "Haciendo {exercise}. ¡Vamos, vamos!"),
    s(SPORT, "Mente sana en cuerpo de mono sano."),
    s(SPORT, "Elongando. Después de programar, lo necesito."),
    s(SPORT, "Llevo {n}0 {exercise}. Bah, {n}."),
    s(SPORT, "El ejercicio me despeja más que el café."),
    s(SPORT, "Me duele todo. Eso significa que funciona."),
    s(SPORT, "Ejercicio a la mañana: el día arranca bien.").h(6, 12),
    s(SPORT, "Me estiro como banana al sol."),
    s(SPORT, "Postura de programador: corregida.").likes(Taste::Sporty, 30),
    t(SPORT, "Una más... ¡una más!"),
    t(SPORT, "Respirar... respirar..."),
    t(SPORT, "Esto lo hago por las bananas de después."),
    t(SPORT, "Ay, la espalda."),
    // ------------------------------------------------------------------
    // Huerta y bananero.
    // ------------------------------------------------------------------
    s(GARDEN, "Regando {plant}. Crecen con amor."),
    s(GARDEN, "Mi huerta es mi mejor proyecto open source."),
    s(GARDEN, "Le hablo a las plantas. Me contestan, creo."),
    s(GARDEN, "Mirá {plant}, ¡qué grandes!"),
    s(GARDEN, "Un poco de tierra en las manos hace bien."),
    s(GARDEN, "Si las plantas tuvieran blockchain..."),
    s(GARDEN, "Sol, agua y paciencia. Como el HODL."),
    s(BANANAS, "¡Voy por bananas! Las mejores del barrio."),
    s(BANANAS, "Este bananero es mi mayor inversión."),
    s(BANANAS, "Cosecha fresca. Nada de supermercado."),
    s(BANANAS, "Banana del árbol a la panza. Directo."),
    t(GARDEN, "Crezcan, chiquitas, crezcan."),
    t(GARDEN, "Ese tomate está casi listo."),
    t(GARDEN, "Qué lindo día para la huerta.").h(8, 18),
    t(BANANAS, "Esa... no, esa otra. ¡Esa!"),
    t(BANANAS, "Está en su punto justo."),
    // ------------------------------------------------------------------
    // Limpiando.
    // ------------------------------------------------------------------
    s(CLEAN, "Limpiando la casa. Barro bugs y polvo."),
    s(CLEAN, "Sábado de limpieza. Mañana, descanso.").sat(),
    s(CLEAN, "Encontré {n} cáscaras de banana bajo el sillón."),
    s(CLEAN, "Casa limpia, mente limpia, código limpio."),
    s(CLEAN, "Barriendo. Es como hacer garbage collection."),
    s(CLEAN, "¿Me ayudás? Ah, no tenés manos. Bueno."),
    s(CLEAN, "Pongo música y limpio en un toque."),
    t(CLEAN, "¿De dónde sale tanto polvo?"),
    t(CLEAN, "Esa esquina también. Y esa."),
    t(CLEAN, "Barrer, barrer, barrer..."),
    // ------------------------------------------------------------------
    // Teléfono.
    // ------------------------------------------------------------------
    s(PHONE, "Hablando con mi vieja. Te manda saludos."),
    s(PHONE, "Ahora no puedo, estoy al teléfono."),
    s(PHONE, "Es {friend}. Dice que vengas el finde."),
    s(PHONE, "Me están vendiendo un seguro. Socorro."),
    s(PHONE, "Llamada del laburo. Pongo voz seria.").wd(),
    s(PHONE, "Llamado de domingo con la familia.").sun(),
    s(PHONE, "Le estoy explicando {coin} a mi abuela."),
    t(PHONE, "Sí... sí... claro... sí..."),
    t(PHONE, "¿Me escuchás? Se corta..."),
    t(PHONE, "Jaja, ¡no te puedo creer!"),
    // ------------------------------------------------------------------
    // Mirando las estrellas.
    // ------------------------------------------------------------------
    s(STARS, "Mirá, ¡{star}! Qué noche más linda."),
    s(STARS, "Ahí arriba debe haber monos programando."),
    s(STARS, "Las estrellas son como los bloques: infinitas."),
    s(STARS, "Pedí un deseo: que {coin} suba. Y paz mundial."),
    s(STARS, "Noche clara. Ideal para pensar en la vida."),
    s(STARS, "Algún día minamos cripto en la Luna."),
    t(STARS, "Qué grande es todo..."),
    t(STARS, "¿Eso fue una estrella fugaz?"),
    t(STARS, "Shh... escuchá el silencio."),
    // ------------------------------------------------------------------
    // Soñando despierto y paseando.
    // ------------------------------------------------------------------
    s(DREAM, "Pensando en la próxima gran idea. Ya viene."),
    s(DREAM, "Mirando por la ventana. Me inspira."),
    s(DREAM, "Me imagino una startup de bananas. Unicornio."),
    s(DREAM, "¿Y si me voy a vivir a la playa con wifi?"),
    s(DREAM, "Pensando. No me hables. Bueno, hablame."),
    s(WANDER, "Dando una vuelta. Las piernas lo piden."),
    s(WANDER, "Estiro las patas entre tarea y tarea."),
    s(WANDER, "Paseando por la casa. Es chica pero es mía."),
    s(WANDER, "Buscando algo... ya me olvidé qué."),
    s(RELAX, "Momento zen. Respiro, sonrío, sigo."),
    s(RELAX, "Sin apuro. Hoy la vida va lento.").we(),
    t(DREAM, "Qué lindo está afuera."),
    t(DREAM, "¿Y si armo mi propia blockchain?"),
    t(DREAM, "Algún día voy a tener un bananero gigante."),
    t(DREAM, "Mmm... nubes con forma de banana."),
    t(WANDER, "¿A qué venía? Ah, a nada."),
    t(WANDER, "Pasito a pasito."),
    t(WANDER, "Esta casa necesita más plantas."),
    // ------------------------------------------------------------------
    // Despertando, medio dormido, durmiendo, de visita.
    // ------------------------------------------------------------------
    s(WAKE, "Buen día... dame cinco minutos más."),
    s(WAKE, "Me levanté. Técnicamente."),
    s(WAKE, "¿Ya es de día? ¿Quién lo autorizó?"),
    s(WAKE, "Gm. Así decimos buen día en cripto."),
    s(WAKE, "Primer pensamiento del día: ¿cómo está {coin}?"),
    s(WAKE, "Me despierto y ya pienso en {feature}.").wd(),
    s(WAKE, "Sin despertador. Amo los fines de semana.").we(),
    t(WAKE, "Ahhh... qué sueño."),
    t(WAKE, "Mate. Necesito mate."),
    t(WAKE, "Los pies. ¿Dónde dejé los pies?"),
    s(GROGGY, "¿Qué hora es? ¿Por qué me despertaste?"),
    s(GROGGY, "Estaba soñando con {coin} a un millón..."),
    s(GROGGY, "Mmm... ¿el servidor se cayó?"),
    s(GROGGY, "Dejame dormir, mañana te explico todo."),
    s(GROGGY, "Zzz... ¿eh? ¿Quién anda ahí?"),
    s(SLEEP, "Zzz... HODL... zzz..."),
    s(SLEEP, "Zzz... compilá... compilá..."),
    s(SLEEP, "Mmm... bananas... zzz..."),
    s(SLEEP, "Zzz... el dip... compro el dip..."),
    s(SLEEP, "*ronquido de mono*"),
    s(SLEEP, "Zzz... un bug más... zzz..."),
    s(SLEEP, "Zzz... game over... otra vez..."),
    s(NAP, "Siestita... cinco minutos..."),
    s(NAP, "Zzz... siesta sagrada... zzz..."),
    s(AWAY, "«Fui a ver a {friend}. Vuelvo enseguida»"),
    s(AWAY, "«Salí a dar una vuelta. Dejé bananas»"),
    s(AWAY, "«De visita. No toquen mi laptop»"),
    s(AWAY, "«Volví en un rato. Regá las plantas»"),
    s(AWAY, "«Me fui a un meetup de cripto»"),
    s(AWAY, "«Fui a comprar yerba. Ya vuelvo»"),
    // ------------------------------------------------------------------
    // Generales: según la hora y el día.
    // ------------------------------------------------------------------
    s(AWAKE, "Gm, gm. ¿Cómo arrancó tu día?").h(6, 11),
    s(AWAKE, "Buen día. Hoy me siento productivo.")
        .h(6, 11)
        .mood(Focused),
    s(AWAKE, "Arranco el día con energía. ¡Vamos!")
        .h(6, 11)
        .mood(Energetic),
    s(AWAKE, "Lunes otra vez. ¿Cómo pasó tan rápido?")
        .wd()
        .h(6, 13)
        .w(3),
    s(AWAKE, "Ya es {weekday}. La semana vuela."),
    s(AWAKE, "Son las {hour}. ¿Ya almorzaste?").h(12, 15),
    s(AWAKE, "Tarde tranquila. Me gusta.").h(14, 19),
    s(AWAKE, "Ya está oscureciendo. Qué rápido.").h(19, 21),
    s(AWAKE, "Ya es tarde... pero un ratito más.").h(23, 3),
    s(AWAKE, "Son las {hour}. Los monos deberían dormir.").h(0, 4),
    s(AWAKE, "Sábado. Sin alarmas, sin reuniones. Paraíso.").sat(),
    s(AWAKE, "Domingo de no hacer nada. Mi favorito.").sun(),
    s(AWAKE, "Finde: modo mono relajado activado.").we(),
    s(AWAKE, "Viernes. Ya huele a finde.").wd().h(14, 24).w(4),
    s(AWAKE, "Hoy estoy de buen humor. ¿Se nota?").mood(Social),
    s(AWAKE, "Hoy no quiero hacer nada. Y está bien.").mood(Lazy),
    s(AWAKE, "Tengo ideas para {n} proyectos nuevos.").mood(Creative),
    s(AWAKE, "Día lindo para estar afuera.")
        .mood(Outdoorsy)
        .h(8, 19),
    s(AWAKE, "¿Viste {coin} hoy? No, mejor no mires."),
    s(AWAKE, "Ser un mono dev es lo mejor que me pasó."),
    s(AWAKE, "¿Sabías que los monos inventaron el blockchain?"),
    s(AWAKE, "Si me necesitás, estoy acá. Siempre."),
    s(AWAKE, "Me gusta que me vengas a ver."),
    s(AWAKE, "Hoy {did}. Productivo, ¿no?"),
    s(AWAKE, "Hace un rato {did}. ¿Qué hiciste vos?"),
    s(AWAKE, "Después {next}. Agenda llena."),
    s(AWAKE, "Ahora esto, y después {next}."),
    s(AWAKE, "Hoy ya {did} y todavía {next}."),
    t(AWAKE, "Me olvidé de algo... ¿{feature}?").wd(),
    t(AWAKE, "Hoy {did}. Nada mal."),
    t(AWAKE, "Después {next}. ¡Qué bien!"),
    t(AWAKE, "¿Cómo estará {coin}?"),
    t(AWAKE, "Qué ganas de una banana."),
    t(AWAKE, "Mmm, ¿y si pruebo {game} hoy?").likes(Taste::Gamer, 50),
    t(AWAKE, "Domingo... qué paz.").sun(),
    t(AWAKE, "Sábado. Todo el día para mí.").sat(),
    // ------------------------------------------------------------------
    // Gustos que se cuelan en cualquier momento.
    // ------------------------------------------------------------------
    s(AWAKE, "¿Jugamos {game} un rato después?").likes(Taste::Gamer, 60),
    s(AWAKE, "Tengo ganas de tocar la guitarra.").likes(Taste::Musical, 60),
    s(AWAKE, "Hoy cocino algo rico. Seguro.").likes(Taste::Foodie, 60),
    s(AWAKE, "Necesito ver a mis amigos pronto.").likes(Taste::Sociable, 60),
    s(AWAKE, "Me iría a correr ahora mismo.").likes(Taste::Sporty, 60),
    s(AWAKE, "Leí un artículo genial sobre {chain}.").likes(Taste::Bookish, 60),
    s(AWAKE, "Un mate y nada más. Eso es vivir.").likes(Taste::Chill, 60),
    s(AWAKE, "La huerta va bien: {plant} crecen un montón.").likes(Taste::GreenThumb, 60),
    // ------------------------------------------------------------------
    // Para que siempre tenga algo que decir sobre lo que hace (huecos
    // generales por grupos de actividades).
    // ------------------------------------------------------------------
    s(WORK, "Laburando. Soy un mono muy ocupado."),
    s(WORK, "El trabajo dignifica. Las bananas también."),
    s(WORK, "Remote work: oficina con vista a mi bananero."),
    s(SEATED_FUN, "Momento de relax en el sillón."),
    s(SEATED_FUN, "Esto es vida. Sillón y algo lindo."),
    t(WORK, "Concentración, concentración..."),
    t(SEATED_FUN, "Qué bien se está acá."),
    t(SLEEP, "Zzz..."),
    t(ANY_ACT, "..."),
    // ==================================================================
    // Segunda tanda: más de todo.
    // ==================================================================
    // Programando: más cultura dev.
    st(CODE, "Hoy aprendí un atajo de teclado nuevo. Poder."),
    st(CODE, "Escribí un test antes del código. Crecí."),
    st(CODE, "Ese warning lo arreglo mañana. O nunca."),
    st(CODE, "La mejor línea de código es la que borrás."),
    st(CODE, "Mi IDE tiene más plugins que bananas tengo."),
    st(CODE, "Modo oscuro activado. Como todo buen dev."),
    st(CODE, "Leí el código de otro. Ahora entiendo mi vida."),
    st(CODE, "Un console.log por acá, otro por allá..."),
    st(CODE, "Git blame dice que fui yo. Git miente."),
    st(CODE, "Resolviendo un merge conflict. Paz interior."),
    st(CODE, "Estoy en la fase de '¿por qué hice esto?'."),
    st(CODE, "Voy a hacer un PR de {big} archivos. Perdón."),
    st(CODE, "Le puse un nombre lindo a la función. Orgullo."),
    st(CODE, "La blockchain no miente. Mi código, a veces."),
    st(CODE, "Estoy escribiendo un oráculo de precios de bananas."),
    st(CODE, "Gas optimizado: {n}0% menos. Aplausos."),
    st(CODE, "Auditoría pasada sin críticos. ¡Fiesta!"),
    st(CODE, "Contrato verificado en el explorer. Precioso."),
    st(CODE, "Tengo una idea para una DAO de monos."),
    st(CODE, "Escribiendo una dApp. La 'd' es de 'dale'."),
    st(CODE, "Pair programming conmigo mismo. Discutimos."),
    st(CODE, "Este algoritmo es O(n). Bueno, O(banana)."),
    st(CODE, "Rubber duck debugging, pero con una banana."),
    st(CODE, "Hoy no rompí producción. Récord personal."),
    st(CODE, "Mis variables tienen nombres de frutas."),
    st(CODE, "Tengo {n} ramas abiertas. Un bosque."),
    st(CODE, "Deployé y el explorer dice 'success'. Lloro."),
    st(CODE, "Escribí la doc. Sí, yo. Sorprendido también."),
    st(CODE, "El código compila en mi máquina. Problema tuyo."),
    st(CODE, "Esto lo hago en una tarde. Dijo el optimista."),
    st(CODE, "Encontré la solución en un issue de 2017."),
    st(CODE, "Más tests, menos sustos. Lo aprendí a golpes."),
    st(CODE, "Cada bug que arreglo me hace más sabio."),
    st(CODE, "Un día voy a escribir un compilador. Un día."),
    st(CODE, "El que inventó los linters me entiende."),
    t(CODE, "Esto con un map queda más lindo..."),
    t(CODE, "¿Por qué hay un TODO de hace dos años?"),
    t(CODE, "Uy, ese nombre de variable es horrible."),
    t(CODE, "Un último cambio y commiteo."),
    t(CODE, "Mmm, ¿esto es thread-safe?"),
    t(CODE, "¡Ah! Ahí estabas, bug."),
    t(CODE, "Me merezco una banana por esto."),
    t(CODE, "Un refactor chiquito... no, mejor no."),
    t(CODE, "Leo, entiendo, escribo, rompo. Repito."),
    t(CODE, "¿Y si le agrego caché?"),
    t(CODE, "Esto va a escalar. Seguro. Creo."),
    t(CODE, "Silencio. Estoy pensando en {lang}."),
    // Reuniones: más.
    st(MEET, "Pantalla compartida. Ojo con las pestañas."),
    st(MEET, "Alguien dijo 'sinergia'. Tomo un sorbo."),
    st(MEET, "Retro del sprint. Lo bueno: bananas gratis."),
    st(MEET, "Estimando tareas. Pongo un {n}. Siempre {n}."),
    st(MEET, "Demo en vivo. Que no falle, que no falle."),
    st(MEET, "Me preguntaron por {feature}. Improvisé bien."),
    t(MEET, "¿Y si apago la cámara un segundito?"),
    t(MEET, "Que alguien diga 'ok, cerramos'."),
    // Mercado: muchas más.
    st(TRADE, "Leyendo el sentimiento del mercado: ansioso."),
    st(TRADE, "Los ballenas se están moviendo. Atento."),
    st(TRADE, "Mi portfolio: 40% {coin}, 60% esperanza."),
    st(TRADE, "Las noticias dicen que todo sube. O baja."),
    st(TRADE, "Puse una alerta de precio. Ahora espero."),
    st(TRADE, "La paciencia es la mejor estrategia cripto."),
    st(TRADE, "Nunca invertir lo que no podés perder. Anotalo."),
    st(TRADE, "Leí un hilo de {n}0 tweets sobre {coin}."),
    st(TRADE, "Me llegó un airdrop de algo raro. No lo toco."),
    st(TRADE, "El halving se viene. Lo siento en los huesos."),
    st(TRADE, "Estoy haciendo staking de {coin}. Rinde algo."),
    st(TRADE, "Mi wallet fría está más fría que el mate."),
    st(TRADE, "Hay que DYOR. Do Your Own Research."),
    st(TRADE, "¿Viste el volumen de {coin}? Raro, raro."),
    st(TRADE, "Este token tiene un perro en el logo. Paso."),
    st(TRADE, "Estoy armando un bot de trading. Con bananas."),
    st(TRADE, "Gráfico de 4 horas: indeciso como yo."),
    st(TRADE, "Las velas japonesas me dan hambre."),
    st(TRADE, "¡{coin} rompió la resistencia! ¡Vamos!").up(),
    st(TRADE, "Dejo correr las ganancias. Con calma.").up(),
    st(TRADE, "Estoy en verde y lo disfruto en silencio.").up(),
    st(TRADE, "¿Saco ganancias? No. ¿Sí? No.").up(),
    st(TRADE, "Los haters no pueden con {coin}.").up(),
    st(TRADE, "Hoy me compro bananas premium.").moon(),
    st(TRADE, "Esto es un bull run de verdad. ¡A la luna!").moon(),
    st(TRADE, "Mi abuela me pregunta por {coin}. Ya está.").moon(),
    st(TRADE, "Lo vi venir. Bueno, no. Pero lo disfruto.").moon(),
    st(TRADE, "Estoy tranquilo. Muy tranquilo. Socorro.").down(),
    st(TRADE, "Fue un mal día. Mañana es otro gráfico.").down(),
    st(TRADE, "A ver si rebota en el soporte...").down(),
    st(TRADE, "Esto es un descuento, ¿no? ¿No?").down(),
    st(TRADE, "Momento de no abrir la app. Disciplina.").down(),
    st(TRADE, "Sangre en las calles. O en los gráficos.").dump(),
    st(TRADE, "Bear market. Hora de construir.").dump(),
    st(TRADE, "Los que construyen en bear ganan en bull.").dump(),
    st(TRADE, "Respiro. Como banana. Sigo HODLeando.").dump(),
    st(TRADE, "Mercado muerto. Ideal para programar.").flat(),
    st(TRADE, "Ni los bots se mueven hoy.").flat(),
    st(TRADE, "Acumulando de a poquito. Sin prisa.").flat(),
    // Leyendo: más.
    st(READ, "Libro sobre criptografía. Me vuela la cabeza."),
    st(READ, "Releo mi libro favorito. Sigue siendo genial."),
    st(READ, "Este libro tiene más plot twists que {coin}."),
    st(READ, "Leyendo sobre Satoshi. ¿Quién será?"),
    st(READ, "Un cómic de monos espaciales. Literatura."),
    st(READ, "Libro de filosofía. Ahora pienso demasiado."),
    t(READ, "Me encanta este personaje."),
    t(READ, "Ese final de capítulo... no puedo parar."),
    t(READ, "Doblo la puntita de la hoja. Perdón, libro."),
    // Comiendo: más.
    st(EAT, "Comida casera. Lo mejor que hay."),
    st(EAT, "Comer mirando por la ventana. Un lujo."),
    st(EAT, "Bananas: potasio, energía y felicidad."),
    st(EAT, "Postre: otra banana. Obvio."),
    st(EAT, "Esto lo cociné yo. Estoy orgulloso."),
    st(EAT, "¿Viste qué bien me quedó la mesa?"),
    t(EAT, "Mastico despacio. Disfruto."),
    t(EAT, "Mmm, le falta un poco de sal."),
    t(EAT, "Una banana de postre y listo."),
    // Cocinando: más.
    st(COOK, "Cocino con música. Sale más rico."),
    st(COOK, "La cocina es mi laboratorio."),
    st(COOK, "Hoy experimento: {dish} con un twist."),
    st(COOK, "La paciencia es el ingrediente secreto."),
    t(COOK, "Esto necesita cinco minutos más."),
    t(COOK, "¿Y si le agrego queso? Siempre queso."),
    // Mate: más.
    st(MATE, "Cebar mate es un arte. Yo soy artista."),
    st(MATE, "Mate con vista al jardín. Lujo total."),
    st(MATE, "Yerba nueva. La pruebo con respeto."),
    st(MATE, "Primer mate del día: el más rico.").h(6, 12),
    st(MATE, "Mate de la tarde. Infaltable.").h(14, 20),
    t(MATE, "Qué paz..."),
    t(MATE, "Este mate me recarga las pilas."),
    st(MATE, "El mate es mi café, mi té y mi terapia."),
    st(MATE, "Mate y {coin} en verde: combo perfecto.").up(),
    st(MATE, "Mate para olvidar el mercado. Rojo, rojo.").down(),
    st(MATE, "Un mate entre deploy y deploy. Lo merezco."),
    st(MATE, "La bombilla tapada otra vez. Clásico."),
    st(MATE, "Mate con tostadas de banana. Un lujo."),
    st(MATE, "Pensando en {feature} con un mate en la mano."),
    st(MATE, "El mate no se apura. Yo tampoco."),
    st(MATE, "¿Viste el termo? Es mi mejor amigo."),
    st(MATE, "Mate, sillón y nada más. Así de simple."),
    st(MATE, "Si {friend} viniera, le cebaba unos mates."),
    st(MATE, "Yerba con hierbas. Hoy me pongo sofisticado."),
    st(MATE, "Mate a las {hour}. Siempre es buena hora."),
    st(MATE, "Me cebo uno y te cuento cómo va {coin}."),
    t(MATE, "Agua a 80 grados. Ni uno más."),
    t(MATE, "Mmm, la yerba está lavada."),
    t(MATE, "Este es el mejor momento del día."),
    t(MATE, "Mate, banana y paz."),
    // Guitarra, baile, consola, peli: más.
    st(GUITAR, "Estoy sacando un tema de oído. Casi."),
    st(GUITAR, "Mis dedos de mono son buenos para esto."),
    st(GUITAR, "Algún día grabo un disco: 'Bananas y Blues'."),
    t(GUITAR, "Ese acorde sonó raro. Me gusta."),
    st(DANCE, "Cuando bailo me olvido del código."),
    st(DANCE, "Inventé un baile para cuando sube {coin}."),
    t(DANCE, "Que no me vea nadie... ¡bah, que me vean!"),
    st(GAMES, "Este juego tiene mejor historia que muchas pelis."),
    st(GAMES, "Juego en difícil. Sufro, pero lo disfruto."),
    st(GAMES, "Encontré un bug en {game}. Deformación profesional."),
    st(GAMES, "Juntando todos los coleccionables. Todos."),
    st(
        GAMES,
        "Un roguelike: morir, aprender, repetir. Como programar.",
    ),
    st(GAMES, "Cooperativo online con {friend}. Es malísimo."),
    st(GAMES, "Me compré un juego retro en oferta. Joya."),
    st(GAMES, "Los juegos de plataformas son mi terapia."),
    t(GAMES, "Ese salto era imposible... ¡lo hice!"),
    t(GAMES, "¿Por qué el boss tiene dos fases?"),
    t(GAMES, "Me falta un ítem. ¿Dónde estará?"),
    st(MOVIE, "Esta peli tiene un hacker que tipea rapidísimo."),
    st(MOVIE, "Película de domingo. La mejor costumbre.").sun(),
    t(MOVIE, "Ese efecto especial es viejísimo. Me encanta."),
    // Ejercicio, huerta, limpieza, teléfono: más.
    st(SPORT, "Después de esto, me merezco una banana."),
    st(SPORT, "El cuerpo también necesita deploys."),
    t(SPORT, "Ya casi... ya casi..."),
    st(GARDEN, "La huerta no tiene bugs. Tiene bichitos."),
    st(GARDEN, "Mis tomates son orgánicos y descentralizados."),
    t(GARDEN, "Agüita, agüita..."),
    st(CLEAN, "Limpiar también es refactorizar la casa."),
    st(CLEAN, "Cuando termine, todo va a brillar."),
    t(CLEAN, "Debajo de la cama no miro. Mejor no."),
    st(PHONE, "Es un amigo que me pide consejo de {coin}."),
    st(PHONE, "Charlando de la vida. Hace falta."),
    t(PHONE, "Sí, sí, el domingo voy."),
    // Relax de fin de semana: lo que más le gusta.
    st(
        RELAX | SEATED_FUN,
        "Fin de semana: el mejor invento del mundo.",
    )
    .we(),
    st(RELAX | SEATED_FUN, "Hoy no abro la laptop. Bueno, quizás.").we(),
    st(RELAX | SEATED_FUN, "Descansar también es productivo.").we(),
    st(RELAX | SEATED_FUN, "Sin reuniones, sin deploys, sin apuro.").we(),
    st(RELAX | SEATED_FUN, "Domingo tranquilo. Así tiene que ser.").sun(),
    st(RELAX | SEATED_FUN, "Sábado lento. Disfruto cada minuto.").sat(),
    st(RELAX | SEATED_FUN, "El lunes que espere. Hoy es mío.").we(),
    st(
        RELAX | SEATED_FUN,
        "Me merezco este descanso. Y una banana.",
    )
    .we(),
    st(RELAX, "Panza arriba, como un mono feliz.")
        .we()
        .mood(Lazy),
    st(RELAX, "No hacer nada es un arte. Soy experto.").mood(Lazy),
    // Humor del día.
    st(AWAKE, "Hoy tengo ganas de crear algo nuevo.").mood(Creative),
    st(AWAKE, "Hoy me quiero mover. ¡Energía!").mood(Energetic),
    st(AWAKE, "Día de enfocarse. Nada me distrae.").mood(Focused),
    st(AWAKE, "Hoy quiero ver gente. O monos.").mood(Social),
    st(AWAKE, "Hoy pide aire libre y sol.").mood(Outdoorsy),
    st(AWAKE, "Hoy estoy en modo perezoso. Me lo permito.").mood(Lazy),
    t(AWAKE, "Se me ocurrió una canción... no, se fue.").mood(Creative),
    t(AWAKE, "¿Y si salgo a correr? ¡Sí!").mood(Energetic),
    t(AWAKE, "Lista de tareas: una menos.").mood(Focused),
    t(AWAKE, "Le escribo a {friend} más tarde.").mood(Social),
    t(AWAKE, "Qué lindo sería un picnic.").mood(Outdoorsy),
    t(AWAKE, "Cinco minutitos más de nada.").mood(Lazy),
    // Generales: cultura cripto y dev.
    s(AWAKE, "¿Sabías que el primer bloque de Bitcoin es de 2009?"),
    s(AWAKE, "Not your keys, not your coins. Regla de oro."),
    s(AWAKE, "Gm. Así saludamos los cripto. Gm."),
    s(AWAKE, "Tengo una seed phrase guardada en tres lugares."),
    s(AWAKE, "Si un mono puede programar, vos también."),
    s(AWAKE, "Mi sueño: que me paguen en bananas tokenizadas."),
    s(AWAKE, "¿Te conté de mi token, $BANANA? Sale pronto."),
    s(AWAKE, "Los bugs no existen, son features sin documentar."),
    s(AWAKE, "Ojo con los links raros. Scam por todos lados."),
    s(AWAKE, "La descentralización es como la banana: para todos."),
    s(AWAKE, "Programar es resolver puzzles todo el día."),
    s(AWAKE, "Los videojuegos me enseñaron a no rendirme."),
    s(AWAKE, "Un buen fin de semana arregla cualquier semana."),
    s(AWAKE, "Lo importante es tener bananas y wifi."),
    t(AWAKE, "Wagmi."),
    t(AWAKE, "¿Habrá salido algún juego nuevo?").likes(Taste::Gamer, 40),
    t(AWAKE, "Tengo que hacer backup de la wallet."),
    t(AWAKE, "Me pica la oreja. ¿Será una idea?"),
    t(AWAKE, "Qué buena casa tengo."),
    t(AWAKE, "Hoy es un buen día."),
    // Por la hora.
    st(AWAKE, "La mañana es para programar. La tarde, para mate.").h(7, 12),
    st(AWAKE, "Ya pasó el mediodía. El día vuela.").h(12, 15),
    st(AWAKE, "La tarde pide un mate y algo dulce.").h(15, 19),
    st(AWAKE, "Ya es de noche. Me pongo cómodo.").h(20, 24),
    st(AWAKE, "Medianoche. Hora de los hackers.").h(0, 2),
    st(AWAKE, "Me tendría que ir a dormir. Tendría.").h(23, 4),
    // Lo que hizo y lo que viene, en más variantes.
    st(AWAKE, "Hoy {did}. Ahora un rato tranqui."),
    st(AWAKE, "¿Sabés qué? Hoy {did}. Re bien."),
    st(AWAKE, "Más tarde {next}. ¿Querés venir?"),
    st(AWAKE, "Lo próximo: {next}. Lo tengo agendado."),
    t(AWAKE, "Hoy {did}... qué día."),
    t(AWAKE, "Y después {next}. Bien."),
];

/// Saludos según el momento (el hueco `{weekday}`, etc. funciona igual).
pub fn greetings(g: Greeting, kind: DayKind) -> &'static [&'static str] {
    let weekend = kind != DayKind::Workday;
    match g {
        Greeting::Hello => &[
            "¡Hola! Soy el mono. Dev, cripto y bananas.",
            "¡Hola! Bienvenido a mi casa. Pasá.",
            "¡Uh, una visita! Hola, soy el mono.",
        ],
        Greeting::Morning if weekend => &[
            "¡Buen día! Hoy no se labura.",
            "Buen día... ¿ya es de mañana? Finde, qué lindo.",
            "¡Gm! Finde de descanso, ¿no?",
            "¡Buen día! Hoy toca relax total.",
        ],
        Greeting::Morning => &[
            "¡Buen día! ¿Mate y a programar?",
            "¡Gm! A darle al {weekday}.",
            "¡Buen día! Hoy deployamos algo, seguro.",
            "¡Buenas! Ya revisé cómo está {coin}.",
            "¡Buen día! Hoy me siento 10x.",
        ],
        Greeting::Afternoon => &[
            "¡Buenas tardes! ¿Cómo va el {weekday}?",
            "¡Hola! Justo pensaba en vos.",
            "¡Buenas! ¿Una banana? Invito.",
            "¡Hola! La tarde viene tranqui.",
        ],
        Greeting::Evening => &[
            "¡Buenas noches! ¿Todo bien?",
            "¡Hola! Ya es de noche, qué rápido.",
            "¡Buenas! Hora de relajar.",
            "¡Hola! Noche tranquila por acá.",
        ],
        Greeting::MissedYou => &[
            "¡Volviste! Te extrañé un montón.",
            "¡Tanto tiempo! ¿Dónde andabas?",
            "¡Hola! Pensé que te habías olvidado de mí.",
            "¡Por fin! Te guardé una banana.",
        ],
        Greeting::Again => &["¿Sí?", "¿Qué pasó?", "¡Acá estoy!", "¿Me llamabas?"],
    }
}

/// Lo que hizo, en pasado (para `{did}`).
pub fn did(a: A) -> Option<&'static str> {
    Some(match a {
        A::Code => "programé un montón",
        A::Meeting => "tuve una reunión eterna",
        A::Trading => "estuve mirando el mercado",
        A::Read => "leí un rato",
        A::Eat | A::Snack => "comí riquísimo",
        A::Cook => "cociné",
        A::Mate => "tomé unos mates",
        A::Guitar => "toqué la guitarra",
        A::Dance => "bailé un rato",
        A::Games => "jugué a la consola",
        A::Movie => "vi una peli",
        A::Exercise => "hice ejercicio",
        A::Stretch => "elongué",
        A::Garden => "regué la huerta",
        A::Bananas => "coseché bananas",
        A::Clean => "limpié toda la casa",
        A::Phone => "hablé por teléfono",
        A::Stargaze => "miré las estrellas",
        A::Away => "fui de visita",
        A::Nap => "dormí la siesta",
        _ => return None,
    })
}

/// Lo que va a hacer (para `{next}`).
pub fn next(a: A) -> Option<&'static str> {
    Some(match a {
        A::Code => "me toca programar",
        A::Meeting => "tengo una reunión",
        A::Trading => "miro el mercado",
        A::Read => "voy a leer un rato",
        A::Eat => "toca comer",
        A::Snack => "toca merendar",
        A::Cook => "voy a cocinar",
        A::Mate => "unos mates",
        A::Guitar => "toco la guitarra",
        A::Dance => "voy a bailar",
        A::Games => "consola",
        A::Movie => "una peli",
        A::Exercise => "a hacer ejercicio",
        A::Stretch => "a elongar",
        A::Garden => "a regar la huerta",
        A::Bananas => "voy por bananas",
        A::Clean => "a limpiar",
        A::Phone => "tengo que llamar a alguien",
        A::Stargaze => "a mirar las estrellas",
        A::Away => "salgo de visita",
        A::Nap => "una siestita",
        A::Sleep => "a dormir",
        _ => return None,
    })
}

pub fn weekday_name(d: Weekday) -> &'static str {
    match d {
        Weekday::Mon => "lunes",
        Weekday::Tue => "martes",
        Weekday::Wed => "miércoles",
        Weekday::Thu => "jueves",
        Weekday::Fri => "viernes",
        Weekday::Sat => "sábado",
        Weekday::Sun => "domingo",
    }
}

/// Los valores de cada hueco.
pub fn pool(slot: &str) -> Option<&'static [&'static str]> {
    Some(match slot {
        "coin" => &[
            "BTC",
            "ETH",
            "SOL",
            "$BANANA",
            "DOGE",
            "$MONO",
            "ADA",
            "mi memecoin",
            "el token del mono",
        ],
        "lang" => &[
            "Rust",
            "Solidity",
            "TypeScript",
            "Go",
            "Move",
            "Python",
            "Haskell",
        ],
        "bug" => &[
            "un off-by-one",
            "un null",
            "una race condition",
            "un reentrancy",
            "un overflow",
            "un typo",
            "un deadlock",
            "un bug fantasma",
        ],
        "tool" => &[
            "el compilador",
            "el borrow checker",
            "Git",
            "el linter",
            "el CI",
            "Docker",
        ],
        "feature" => &[
            "el login",
            "el swap",
            "el staking",
            "la wallet",
            "el bridge",
            "el dashboard",
            "el airdrop",
            "el marketplace",
        ],
        "chain" => &[
            "Ethereum",
            "Solana",
            "una L2",
            "la testnet",
            "mainnet",
            "mi nodo",
        ],
        "game" => &[
            "Zelda",
            "un roguelike",
            "Mario Kart",
            "un metroidvania",
            "Minecraft",
            "Tetris",
            "Hollow Knight",
            "un juego retro",
            "Stardew Valley",
            "Celeste",
        ],
        "food" => &[
            "banana",
            "licuado de banana",
            "banana split",
            "tostadas con banana",
            "panqueques",
            "milanesa",
            "ensalada de frutas",
            "fideos",
            "empanadas",
        ],
        "dish" => &[
            "un guiso",
            "fideos con tuco",
            "una sopa",
            "un risotto",
            "milanesas",
            "un curry de banana",
            "un pan de banana",
        ],
        "movie" => &[
            "una de ciencia ficción",
            "una de hackers",
            "una de robots",
            "un clásico",
            "una de terror",
            "una animada",
            "un documental",
        ],
        // Sin artículo: van después de "a", "con", "es".
        "friend" => &[
            "Don Carpincho",
            "Rita la ardilla",
            "Pepe el loro",
            "mi primo Gorila",
            "Tito el tucán",
            "Doña Tortuga",
        ],
        "plant" => &[
            "los tomates",
            "la albahaca",
            "las lechugas",
            "las zanahorias",
            "las frutillas",
        ],
        "song" => &[
            "un rock nacional",
            "una zamba",
            "un blues",
            "una chacarera",
            "un tema propio",
        ],
        "star" => &[
            "la Cruz del Sur",
            "las Tres Marías",
            "Marte",
            "la Luna",
            "Venus",
        ],
        "exercise" => &["sentadillas", "flexiones", "saltos", "abdominales"],
        _ => return None,
    })
}
