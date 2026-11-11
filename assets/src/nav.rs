//! Navegación por el hábitat: caminos que esquivan los muebles.
//!
//! `tools/build_nav.py` arma, desde la escena, una grilla de 5 cm con los
//! obstáculos ya agrandados por el radio del personaje (basta con mover su
//! centro por celdas libres) y una red de nodos unidos por rectas libres.
//! Un camino se busca en la red (Dijkstra, pocos nodos, sin memoria
//! dinámica) y después se estira: de cada punto se salta al más lejano que
//! se ve en línea recta.
//!
//! Coordenadas: el plano del piso del motor, `[x, z]` en Q8 (1024 = 1 m).

use engine3d::fixed::isqrt32;

/// Nodos máximos de la red (tamaño de las tablas de Dijkstra).
pub const MAX_NODES: usize = 320;
/// Puntos máximos de un camino ya estirado.
pub const MAX_PATH: usize = 24;
/// Distancia máxima para engancharse a la red desde un punto cualquiera.
const ATTACH: i32 = 1200;
/// Lo que un tramo puede tener dentro de un obstáculo en cada punta (Q8).
pub const EDGE: i32 = 360;

pub type Point = [i32; 2];

pub struct NavMap {
    /// Esquina de la grilla (Q8) y celdas por metro.
    pub x0: i32,
    pub z0: i32,
    pub per_m: i32,
    pub w: usize,
    pub h: usize,
    /// Un bit por celda (fila a fila en z): 1 = no se puede pisar.
    pub blocked: &'static [u8],
    pub nodes: &'static [Point],
    /// Vecinos del nodo `i`: `links[first[i]..first[i + 1]]`.
    pub first: &'static [u16],
    pub links: &'static [u16],
}

pub fn dist(a: Point, b: Point) -> i32 {
    let (dx, dz) = ((a[0] - b[0]) as i64, (a[1] - b[1]) as i64);
    isqrt32((dx * dx + dz * dz).min(u32::MAX as i64) as u32) as i32
}

impl NavMap {
    fn cell(&self, p: Point) -> Option<(usize, usize)> {
        let (dx, dz) = (p[0] - self.x0, p[1] - self.z0);
        if dx < 0 || dz < 0 {
            return None;
        }
        let (i, j) = (
            ((dx * self.per_m) >> 10) as usize,
            ((dz * self.per_m) >> 10) as usize,
        );
        (i < self.w && j < self.h).then_some((i, j))
    }

    /// ¿Puede estar parado ahí (su centro)?
    pub fn is_free(&self, p: Point) -> bool {
        match self.cell(p) {
            Some((i, j)) => {
                let k = j * self.w + i;
                self.blocked[k / 8] & (1 << (k % 8)) == 0
            }
            None => false,
        }
    }

    /// ¿Se puede ir en línea recta de `a` a `b`? Se permite que los
    /// extremos estén "dentro" de un obstáculo agrandado (parado delante de
    /// una silla, pegado a la mesa) hasta [`EDGE`] de cada punta, siempre que
    /// el tramo salga de ahí y no vuelva a entrar en otro hasta llegar.
    pub fn clear(&self, a: Point, b: Point) -> bool {
        // Muestras cada ~2,5 cm (media celda).
        let step = (512 / self.per_m).max(1);
        let len = dist(a, b);
        let n = (len / step).max(1);
        let edge = (EDGE as i64 * n as i64 / len.max(1) as i64) as i32;
        // 0: en el obstáculo de salida, 1: libre, 2: en el de llegada.
        let mut state = 0;
        for k in 0..=n {
            let p = [
                a[0] + ((b[0] - a[0]) as i64 * k as i64 / n as i64) as i32,
                a[1] + ((b[1] - a[1]) as i64 * k as i64 / n as i64) as i32,
            ];
            let free = self.is_free(p);
            state = match (state, free) {
                (0, true) | (1, true) => 1,
                (0, false) if k <= edge => 0,
                (1, false) | (2, false) if k >= n - edge => 2,
                _ => return false,
            };
        }
        true
    }

    /// ¿La recta va por celdas libres de punta a punta?
    pub fn clear_strict(&self, a: Point, b: Point) -> bool {
        let step = (512 / self.per_m).max(1);
        let n = (dist(a, b) / step).max(1);
        (0..=n).all(|k| {
            self.is_free([
                a[0] + ((b[0] - a[0]) as i64 * k as i64 / n as i64) as i32,
                a[1] + ((b[1] - a[1]) as i64 * k as i64 / n as i64) as i32,
            ])
        })
    }

    /// Camino de `from` a `to` esquivando obstáculos, sin incluir `from`.
    /// Devuelve cuántos puntos escribió en `out` (0 si no hay camino).
    pub fn path(&self, from: Point, to: Point, out: &mut [Point; MAX_PATH]) -> usize {
        if self.clear(from, to) {
            out[0] = to;
            return 1;
        }
        let n = self.nodes.len().min(MAX_NODES);
        let mut d = [u32::MAX; MAX_NODES];
        let mut prev = [u16::MAX; MAX_NODES];
        let mut done = [false; MAX_NODES];
        // Arranque: todos los nodos que se ven desde `from`; si ninguno, el
        // más cercano (mejor un tramo dudoso que quedarse quieto).
        let mut any = false;
        for (i, &p) in self.nodes[..n].iter().enumerate() {
            let dd = dist(from, p);
            if dd <= ATTACH && self.clear(from, p) {
                d[i] = dd as u32;
                any = true;
            }
        }
        if !any && let Some(i) = self.nearest(from) {
            d[i] = dist(from, self.nodes[i]) as u32;
        }
        // Llegada: nodos desde los que se ve `to`.
        let mut goal_cost = [u32::MAX; MAX_NODES];
        let mut any_goal = false;
        for (i, &p) in self.nodes[..n].iter().enumerate() {
            let dd = dist(p, to);
            if dd <= ATTACH && self.clear(p, to) {
                goal_cost[i] = dd as u32;
                any_goal = true;
            }
        }
        if !any_goal && let Some(i) = self.nearest(to) {
            goal_cost[i] = dist(self.nodes[i], to) as u32;
        }

        // Dijkstra simple (O(n²)): con unos cientos de nodos alcanza.
        let (mut best, mut best_node) = (u32::MAX, usize::MAX);
        loop {
            let mut u = usize::MAX;
            for i in 0..n {
                if !done[i] && d[i] != u32::MAX && (u == usize::MAX || d[i] < d[u]) {
                    u = i;
                }
            }
            if u == usize::MAX || d[u] >= best {
                break;
            }
            done[u] = true;
            if goal_cost[u] != u32::MAX && d[u] + goal_cost[u] < best {
                best = d[u] + goal_cost[u];
                best_node = u;
            }
            let (a, b) = (self.first[u] as usize, self.first[u + 1] as usize);
            for &v in &self.links[a..b] {
                let v = v as usize;
                let nd = d[u] + dist(self.nodes[u], self.nodes[v]) as u32;
                if nd < d[v] {
                    d[v] = nd;
                    prev[v] = u as u16;
                }
            }
        }
        if best_node == usize::MAX {
            return 0;
        }

        // Nodos del camino, del final al principio.
        let mut rev = [[0i32; 2]; MAX_PATH];
        let mut len = 0;
        rev[len] = to;
        len += 1;
        let mut k = best_node;
        while k != u16::MAX as usize && len < MAX_PATH {
            rev[len] = self.nodes[k];
            len += 1;
            k = prev[k] as usize;
        }
        // Estirar: desde cada punto, saltar al más lejano que se vea.
        let pts = &mut rev[..len];
        pts.reverse();
        let mut count = 0;
        let mut at = from;
        let mut i = 0;
        while i < pts.len() && count < MAX_PATH {
            let mut j = pts.len() - 1;
            while j > i && !self.clear(at, pts[j]) {
                j -= 1;
            }
            out[count] = pts[j];
            count += 1;
            at = pts[j];
            i = j + 1;
        }
        count
    }

    fn nearest(&self, p: Point) -> Option<usize> {
        (0..self.nodes.len().min(MAX_NODES)).min_by_key(|&i| dist(p, self.nodes[i]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::house::HOUSE;

    fn xz(name: &str) -> Point {
        let s = HOUSE.spot(name).unwrap_or_else(|| panic!("falta {name}"));
        [s.pos[0], s.pos[2]]
    }

    #[test]
    fn the_graph_only_uses_free_straight_lines() {
        let nav = HOUSE.nav;
        assert!(nav.nodes.len() <= MAX_NODES);
        for (i, &p) in nav.nodes.iter().enumerate() {
            for &j in &nav.links[nav.first[i] as usize..nav.first[i + 1] as usize] {
                assert!(nav.clear_strict(p, nav.nodes[j as usize]), "{i}-{j}");
            }
        }
    }

    #[test]
    fn every_waypoint_reaches_every_other_without_crossing_furniture() {
        let nav = HOUSE.nav;
        let names: Vec<&str> = HOUSE.waypoints.iter().map(|w| w.name).collect();
        let mut out = [[0; 2]; MAX_PATH];
        for &a in &names {
            for &b in &names {
                let (from, to) = (xz(a), xz(b));
                let n = nav.path(from, to, &mut out);
                assert!(n > 0, "sin camino de {a} a {b}");
                assert_eq!(out[n - 1], to);
                let mut at = from;
                for &p in &out[..n] {
                    assert!(
                        nav.clear(at, p),
                        "{a} -> {b}: el tramo {at:?} -> {p:?} cruza algo"
                    );
                    at = p;
                }
            }
        }
    }

    #[test]
    fn walking_through_the_desk_is_not_allowed() {
        // Del pasillo de la oficina al fondo, del otro lado del escritorio:
        // la recta lo atraviesa, el camino lo rodea.
        let nav = HOUSE.nav;
        let c = xz("spot_desk");
        let (from, to) = ([c[0], c[1] - 1100], [c[0], c[1] + 400]);
        assert!(!nav.clear(from, to));
        let mut out = [[0; 2]; MAX_PATH];
        let n = nav.path(from, to, &mut out);
        assert!(n >= 2, "tiene que rodear el escritorio: {:?}", &out[..n]);
    }
}
