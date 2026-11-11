//! Cliente de Stratum v1, el protocolo de los pools: JSON de una línea por
//! mensaje sobre TCP.
//!
//! 1. `mining.subscribe` → el pool da el `extranonce1` (una parte de la
//!    transacción de premio que es solo nuestra) y el largo del `extranonce2`
//!    (la parte que elegimos nosotros).
//! 2. `mining.authorize` con `direccion.nombre`: en un pool solo, la
//!    dirección es a dónde va el premio.
//! 3. `mining.notify` trae cada trabajo: con él se arma la transacción de
//!    premio, la raíz de Merkle y la cabecera del bloque.
//! 4. `mining.submit` manda cada nonce que cumple la dificultad del pool
//!    (`mining.set_difficulty`). Esos "shares" no pagan nada en un pool solo:
//!    solo sirven para mostrar que el minero trabaja.
//!
//! Este módulo no toca la red: recibe líneas y escribe las respuestas.

use core::fmt::{self, Write};

use heapless::{String, Vec};
use sha2::{Digest, Sha256};

use crate::json::Json;
use crate::work::{self, Work, hex_decode, sha256d, target_from_difficulty};

/// Dificultad que se le pide al pool: con decenas de kH/s da un share cada
/// pocos segundos, así se ve que anda (si el pool lo acepta).
pub const SUGGESTED_DIFFICULTY: f64 = 0.0005;

const ID_SUBSCRIBE: u64 = 1;
const ID_AUTHORIZE: u64 = 2;
const ID_SUGGEST: u64 = 3;
const FIRST_SUBMIT_ID: u64 = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: String<64>,
    /// En el orden de la cabecera.
    prevhash: [u8; 32],
    coinb1: Vec<u8, 512>,
    coinb2: Vec<u8, 512>,
    branches: Vec<[u8; 32], 16>,
    version: u32,
    nbits: u32,
    ntime: u32,
}

/// Un trabajo para el hash y lo necesario para mandar lo que encuentre.
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub work: Work,
    pub job_id: String<64>,
    pub extranonce2: u64,
    pub ntime: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Nothing,
    Subscribed,
    Authorized(bool),
    /// Trabajo nuevo; `clean` = el anterior ya no sirve (cambió el bloque).
    NewJob {
        clean: bool,
    },
    Difficulty(f64),
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub accepted: u32,
    pub rejected: u32,
    /// La mejor dificultad encontrada (el "casi" más cercano a un bloque).
    pub best: f64,
    pub jobs: u32,
}

pub struct Client {
    user: String<128>,
    next_id: u64,
    extranonce1: Vec<u8, 16>,
    en2_size: usize,
    en2: u64,
    difficulty: f64,
    job: Option<Job>,
    pub stats: Stats,
}

impl Client {
    /// `address.worker`, como lo listan los pools.
    pub fn new(address: &str, worker: &str) -> Client {
        let mut user = String::new();
        let _ = write!(user, "{address}.{worker}");
        Client {
            user,
            next_id: FIRST_SUBMIT_ID,
            extranonce1: Vec::new(),
            en2_size: 4,
            en2: 0,
            difficulty: 1.0,
            job: None,
            stats: Stats::default(),
        }
    }

    pub fn difficulty(&self) -> f64 {
        self.difficulty
    }

    /// Lo primero que se manda al conectarse. `suggested` es la dificultad de
    /// share que se le pide al pool.
    pub fn hello(&self, suggested: f64, out: &mut impl Write) -> fmt::Result {
        write!(
            out,
            "{{\"id\":{ID_SUBSCRIBE},\"method\":\"mining.subscribe\",\"params\":[\"mono/0.1\"]}}\n\
             {{\"id\":{ID_AUTHORIZE},\"method\":\"mining.authorize\",\"params\":[\"{}\",\"x\"]}}\n\
             {{\"id\":{ID_SUGGEST},\"method\":\"mining.suggest_difficulty\",\"params\":[{suggested}]}}\n",
            self.user
        )
    }

    /// Procesa una línea del pool.
    pub fn handle(&mut self, line: &str) -> Event {
        let Some(msg) = Json::parse(line) else {
            return Event::Nothing;
        };
        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
            let Some(params) = msg.get("params") else {
                return Event::Nothing;
            };
            return match method {
                "mining.notify" => self.notify(params),
                "mining.set_difficulty" => match params.at(0).and_then(|d| d.as_f64()) {
                    Some(d) if d > 0.0 => {
                        self.difficulty = d;
                        Event::Difficulty(d)
                    }
                    _ => Event::Nothing,
                },
                "mining.set_extranonce" => {
                    self.set_extranonce(params.at(0), params.at(1));
                    Event::Nothing
                }
                _ => Event::Nothing,
            };
        }

        let id = msg.get("id").and_then(|i| i.as_u64());
        let result = msg.get("result");
        let ok = result.and_then(|r| r.as_bool()) == Some(true);
        match id {
            Some(ID_SUBSCRIBE) => {
                let Some(r) = result else {
                    return Event::Nothing;
                };
                self.set_extranonce(r.at(1), r.at(2));
                Event::Subscribed
            }
            Some(ID_AUTHORIZE) => Event::Authorized(ok),
            Some(n) if n >= FIRST_SUBMIT_ID => {
                if ok {
                    self.stats.accepted += 1;
                    Event::Accepted
                } else {
                    self.stats.rejected += 1;
                    Event::Rejected
                }
            }
            _ => Event::Nothing,
        }
    }

    fn set_extranonce(&mut self, en1: Option<Json>, size: Option<Json>) {
        let mut buf = [0u8; 16];
        if let Some(n) = en1
            .and_then(|e| e.as_str())
            .and_then(|h| hex_decode(h, &mut buf))
        {
            self.extranonce1 = Vec::from_slice(&buf[..n]).unwrap_or_default();
        }
        if let Some(size) = size.and_then(|s| s.as_u64()) {
            self.en2_size = (size as usize).clamp(1, 8);
        }
    }

    fn notify(&mut self, p: Json) -> Event {
        let Some(job) = parse_job(p) else {
            return Event::Nothing;
        };
        let clean = p.at(8).and_then(|c| c.as_bool()).unwrap_or(false) || self.job.is_none();
        self.job = Some(job);
        self.stats.jobs += 1;
        Event::NewJob { clean }
    }

    /// Arma un trabajo nuevo (con otro `extranonce2`) para el último job.
    pub fn next_work(&mut self) -> Option<Assignment> {
        let job = self.job.as_ref()?;
        self.en2 = self.en2.wrapping_add(1);
        let en2 = self.en2.to_be_bytes();
        let en2 = &en2[8 - self.en2_size..];

        let mut h = Sha256::new();
        h.update(&job.coinb1);
        h.update(&self.extranonce1);
        h.update(en2);
        h.update(&job.coinb2);
        let mut root: [u8; 32] = Sha256::digest(h.finalize()).into();
        for b in &job.branches {
            let mut pair = [0u8; 64];
            pair[..32].copy_from_slice(&root);
            pair[32..].copy_from_slice(b);
            root = sha256d(&pair);
        }

        let mut header = [0u8; 80];
        header[..4].copy_from_slice(&job.version.to_le_bytes());
        header[4..36].copy_from_slice(&job.prevhash);
        header[36..68].copy_from_slice(&root);
        header[68..72].copy_from_slice(&job.ntime.to_le_bytes());
        header[72..76].copy_from_slice(&job.nbits.to_le_bytes());
        Some(Assignment {
            work: Work::from_header(&header, target_from_difficulty(self.difficulty)),
            job_id: job.id.clone(),
            extranonce2: self.en2,
            ntime: job.ntime,
        })
    }

    /// ¿Sigue sirviendo este trabajo? (no llegó un job que lo reemplace)
    pub fn is_current(&self, a: &Assignment) -> bool {
        self.job.as_ref().is_some_and(|j| j.id == a.job_id)
    }

    /// Manda un nonce que cumplió `a.work.share_target`.
    pub fn submit(
        &mut self,
        a: &Assignment,
        nonce: u32,
        hash: &[u8; 32],
        out: &mut impl Write,
    ) -> fmt::Result {
        let d = work::difficulty(hash);
        if d > self.stats.best {
            self.stats.best = d;
        }
        let id = self.next_id;
        self.next_id += 1;
        write!(
            out,
            "{{\"id\":{id},\"method\":\"mining.submit\",\"params\":[\"{}\",\"{}\",\"",
            self.user, a.job_id
        )?;
        for i in (0..self.en2_size).rev() {
            write!(out, "{:02x}", (a.extranonce2 >> (8 * i)) as u8)?;
        }
        writeln!(out, "\",\"{:08x}\",\"{nonce:08x}\"]}}", a.ntime)
    }

    /// Registra un hash que no llegó a share (para "el mejor tiro").
    pub fn note(&mut self, hash: &[u8; 32]) {
        let d = work::difficulty(hash);
        if d > self.stats.best {
            self.stats.best = d;
        }
    }
}

fn parse_job(p: Json) -> Option<Job> {
    let hex32 = |j: Json| -> Option<[u8; 32]> {
        let mut out = [0u8; 32];
        (hex_decode(j.as_str()?, &mut out)? == 32).then_some(out)
    };
    let hex_vec = |j: Json| -> Option<Vec<u8, 512>> {
        let mut buf = [0u8; 512];
        let n = hex_decode(j.as_str()?, &mut buf)?;
        Vec::from_slice(&buf[..n]).ok()
    };
    let hex_u32 = |j: Json| u32::from_str_radix(j.as_str()?, 16).ok();

    // El prevhash viene en palabras de 4 bytes dadas vuelta.
    let mut prevhash = hex32(p.at(1)?)?;
    for word in prevhash.as_chunks_mut::<4>().0 {
        word.reverse();
    }
    let mut branches = Vec::new();
    for b in p.at(4)?.items() {
        branches.push(hex32(b)?).ok()?;
    }
    Some(Job {
        id: crate::config::text(p.at(0)?.as_str()?)?,
        prevhash,
        coinb1: hex_vec(p.at(2)?)?,
        coinb2: hex_vec(p.at(3)?)?,
        branches,
        version: hex_u32(p.at(5)?)?,
        nbits: hex_u32(p.at(6)?)?,
        ntime: hex_u32(p.at(7)?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::meets;

    const ADDR: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    #[test]
    fn hello_is_three_json_lines() {
        let c = Client::new(ADDR, "mono");
        let mut out: String<512> = String::new();
        c.hello(SUGGESTED_DIFFICULTY, &mut out).unwrap();
        let lines: std::vec::Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        for l in &lines {
            assert!(Json::parse(l).is_some(), "{l}");
        }
        assert!(lines[1].contains(&std::format!("\"{ADDR}.mono\"")));
    }

    fn subscribed() -> Client {
        let mut c = Client::new(ADDR, "mono");
        assert_eq!(
            c.handle(r#"{"id":1,"result":[[["mining.notify","ae6812eb4cd7735a302a8a9dd95cf71f"]],"08000002",4],"error":null}"#),
            Event::Subscribed
        );
        assert_eq!(
            c.handle(r#"{"id":2,"result":true,"error":null}"#),
            Event::Authorized(true)
        );
        assert_eq!(
            c.handle(r#"{"id":null,"method":"mining.set_difficulty","params":[0.5]}"#),
            Event::Difficulty(0.5)
        );
        c
    }

    const NOTIFY: &str = r#"{"params": ["bf", "4d16b6f85af6e2198f44ae2a6de67f78487ae5611b77c6c0440b921e00000000",
        "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff20020862062f503253482f04b8864e5008",
        "072f736c7573682f000000000100f2052a010000001976a914d23fcdf86f7e756a64a7a9688ef9903327048ed988ac00000000", [],
        "00000002", "1c2ac4af", "504e86b9", false], "id": null, "method": "mining.notify"}"#;

    #[test]
    fn builds_the_header_from_a_notify() {
        let mut c = subscribed();
        assert_eq!(c.handle(NOTIFY), Event::NewJob { clean: true });
        let a = c.next_work().unwrap();
        assert_eq!(a.job_id, "bf");
        assert_eq!(a.extranonce2, 1);
        assert_eq!(a.ntime, 0x504E_86B9);

        // Lo mismo, a mano.
        let mut coinbase = std::vec::Vec::new();
        let mut buf = [0u8; 512];
        for h in [
            "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff20020862062f503253482f04b8864e5008",
            "08000002",
            "00000001",
            "072f736c7573682f000000000100f2052a010000001976a914d23fcdf86f7e756a64a7a9688ef9903327048ed988ac00000000",
        ] {
            let n = hex_decode(h, &mut buf).unwrap();
            coinbase.extend_from_slice(&buf[..n]);
        }
        let root = sha256d(&coinbase);
        let mut header = [0u8; 80];
        header[..4].copy_from_slice(&2u32.to_le_bytes());
        let mut prev = [0u8; 32];
        hex_decode(
            "4d16b6f85af6e2198f44ae2a6de67f78487ae5611b77c6c0440b921e00000000",
            &mut prev,
        );
        for w in prev.as_chunks_mut::<4>().0 {
            w.reverse();
        }
        header[4..36].copy_from_slice(&prev);
        header[36..68].copy_from_slice(&root);
        header[68..72].copy_from_slice(&0x504E_86B9u32.to_le_bytes());
        header[72..76].copy_from_slice(&0x1C2A_C4AFu32.to_le_bytes());
        header[76..].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(a.work.hash(7), sha256d(&header));
        assert_eq!(a.work.share_target, target_from_difficulty(0.5));

        // El siguiente trabajo cambia el extranonce2 (y la raíz).
        let b = c.next_work().unwrap();
        assert_eq!(b.extranonce2, 2);
        assert_ne!(b.work.midstate, a.work.midstate);
    }

    #[test]
    fn finds_a_share_and_submits_it() {
        let mut c = subscribed();
        c.handle(NOTIFY);
        let a = c.next_work().unwrap();
        // Con dificultad 0.5, uno de cada ~2^31 nonces: buscar uno fácil.
        let easy = target_from_difficulty(1.0 / 65536.0);
        let (nonce, hash) = (0u32..)
            .map(|n| (n, a.work.hash(n)))
            .find(|(_, h)| meets(h, &easy))
            .unwrap();
        let mut out: String<512> = String::new();
        c.submit(&a, nonce, &hash, &mut out).unwrap();
        let j = Json::parse(out.trim_end()).unwrap();
        assert_eq!(j.get("id").unwrap().as_u64(), Some(FIRST_SUBMIT_ID));
        let p = j.get("params").unwrap();
        assert_eq!(p.at(1).unwrap().as_str(), Some("bf"));
        assert_eq!(p.at(2).unwrap().as_str(), Some("00000001"));
        assert_eq!(p.at(3).unwrap().as_str(), Some("504e86b9"));
        assert_eq!(
            p.at(4)
                .unwrap()
                .as_str()
                .map(|s| u32::from_str_radix(s, 16).unwrap()),
            Some(nonce)
        );
        assert!(c.stats.best >= 1.0 / 65536.0);

        assert_eq!(
            c.handle(r#"{"id":10,"result":true,"error":null}"#),
            Event::Accepted
        );
        assert_eq!(
            c.handle(r#"{"id":11,"result":null,"error":[23,"Low difficulty share",null]}"#),
            Event::Rejected
        );
        assert_eq!((c.stats.accepted, c.stats.rejected), (1, 1));
    }

    #[test]
    fn a_new_job_replaces_the_old_work() {
        let mut c = subscribed();
        c.handle(NOTIFY);
        let a = c.next_work().unwrap();
        assert!(c.is_current(&a));
        let next = NOTIFY
            .replace("\"bf\"", "\"c0\"")
            .replace("false]", "true]");
        assert_eq!(c.handle(&next), Event::NewJob { clean: true });
        assert!(!c.is_current(&a));
    }
}
