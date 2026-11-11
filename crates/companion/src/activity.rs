//! Lo que puede estar haciendo el personaje. Cada actividad tiene su lugar
//! en el hábitat y su animación (ver el crate `habitat`).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Activity {
    Sleep,
    /// Recién levantado: se estira, bosteza.
    WakeUp,
    /// Desayuno, almuerzo y cena.
    Eat,
    /// La merienda de la tarde.
    Snack,
    Nap,
    Exercise,
    Read,
    /// Programa en la laptop (es programador blockchain).
    Code,
    /// Sale al bananero, cosecha una banana y vuelve a comerla.
    Bananas,
    Garden,
    /// Camina por el cuadro, mira cosas.
    Wander,
    /// Mira por la ventana, pensativo.
    Daydream,
    /// Se fue de visita a otro cuadro: el marco queda vacío.
    Away,
    /// Lo despertaste: medio dormido hasta que vuelve a la cama.
    Groggy,
    /// Videollamada de trabajo en la laptop: habla y gesticula.
    Meeting,
    /// Mira los gráficos del mercado en el monitor, mano en el mentón.
    Trading,
    /// Toma mate tranquilo en el sillón.
    Mate,
    /// Cocina en la hornalla, revolviendo la olla.
    Cook,
    /// Toca la guitarra sentado en el borde de la cama.
    Guitar,
    Dance,
    /// Elonga en la colchoneta.
    Stretch,
    /// Habla por teléfono caminando por la casa.
    Phone,
    /// Barre la casa.
    Clean,
    /// De noche, en el jardín, mira el cielo.
    Stargaze,
    /// Juega a la consola en el sillón (la tele queda del lado de la cámara).
    Games,
    /// Mira una película en el sillón.
    Movie,
}

impl Activity {
    pub const ALL: [Activity; 26] = [
        Activity::Sleep,
        Activity::WakeUp,
        Activity::Eat,
        Activity::Snack,
        Activity::Nap,
        Activity::Exercise,
        Activity::Read,
        Activity::Code,
        Activity::Bananas,
        Activity::Garden,
        Activity::Wander,
        Activity::Daydream,
        Activity::Away,
        Activity::Groggy,
        Activity::Meeting,
        Activity::Trading,
        Activity::Mate,
        Activity::Cook,
        Activity::Guitar,
        Activity::Dance,
        Activity::Stretch,
        Activity::Phone,
        Activity::Clean,
        Activity::Stargaze,
        Activity::Games,
        Activity::Movie,
    ];

    pub fn is_asleep(self) -> bool {
        matches!(self, Activity::Sleep | Activity::Nap)
    }

    pub fn is_present(self) -> bool {
        self != Activity::Away
    }

    /// ¿Es parte de su trabajo?
    pub fn is_work(self) -> bool {
        matches!(self, Activity::Code | Activity::Meeting | Activity::Trading)
    }

    pub fn label(self) -> &'static str {
        match self {
            Activity::Sleep => "Durmiendo",
            Activity::WakeUp => "Despertándose",
            Activity::Eat => "Comiendo",
            Activity::Snack => "Merendando",
            Activity::Nap => "Siesta",
            Activity::Exercise => "Ejercicio",
            Activity::Read => "Leyendo",
            Activity::Code => "Programando",
            Activity::Bananas => "Buscando bananas",
            Activity::Garden => "Regando plantas",
            Activity::Wander => "Paseando",
            Activity::Daydream => "Soñando despierto",
            Activity::Away => "De visita",
            Activity::Groggy => "Medio dormido",
            Activity::Meeting => "En una reunión",
            Activity::Trading => "Mirando el mercado",
            Activity::Mate => "Tomando mate",
            Activity::Cook => "Cocinando",
            Activity::Guitar => "Tocando la guitarra",
            Activity::Dance => "Bailando",
            Activity::Stretch => "Elongando",
            Activity::Phone => "Hablando por teléfono",
            Activity::Clean => "Limpiando",
            Activity::Stargaze => "Mirando las estrellas",
            Activity::Games => "Jugando a la consola",
            Activity::Movie => "Viendo una película",
        }
    }

    /// Color para dibujar la agenda (paneles de depuración).
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Activity::Sleep | Activity::Nap => (40, 50, 110),
            Activity::Groggy => (90, 90, 140),
            Activity::WakeUp => (250, 200, 120),
            Activity::Eat | Activity::Snack => (230, 120, 70),
            Activity::Cook => (240, 150, 60),
            Activity::Mate => (150, 190, 80),
            Activity::Bananas => (244, 208, 64),
            Activity::Exercise | Activity::Stretch => (220, 60, 70),
            Activity::Dance => (240, 90, 170),
            Activity::Read => (150, 110, 200),
            Activity::Code => (90, 160, 255),
            Activity::Meeting => (60, 120, 220),
            Activity::Trading => (40, 200, 200),
            Activity::Garden => (90, 180, 90),
            Activity::Wander | Activity::Phone => (120, 200, 210),
            Activity::Clean => (180, 170, 140),
            Activity::Daydream | Activity::Stargaze => (200, 200, 230),
            Activity::Guitar => (200, 140, 90),
            Activity::Games | Activity::Movie => (170, 90, 220),
            Activity::Away => (70, 70, 70),
        }
    }
}
