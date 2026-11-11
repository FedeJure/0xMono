//! La RAM del ESP32-C3 no alcanza para la casa en 3D y el WiFi a la vez: el
//! renderer ocupa ~165 KB y el WiFi pide ~100 KB de memoria dinámica. Por eso
//! cada modo arranca con un reinicio y usa la misma memoria para otra cosa:
//!
//! - **La casa** usa el renderer (en la RAM principal) y, en la RAM que le
//!   devuelve el cargador de arranque ([`Arena`], 64 KB), las franjas de color
//!   y profundidad, el hábitat y el juego.
//! - **Los modos con WiFi** no tocan nada de eso: el renderer y el [`Arena`]
//!   pasan a ser la memoria dinámica ([`give_to_heap`]).

use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

use engine3d::{Camera, Renderer};
use esp_hal::ram;
use static_cell::ConstStaticCell;

pub type R = Renderer<4608, 2816>;

// Se arma en tiempo de compilación: construirlo en la pila al arrancar la
// desborda.
static RENDERER: ConstStaticCell<R> = ConstStaticCell::new(R::new(
    Camera::new(240, 240, 300, 96),
    assets::scene::light(),
));

pub const ARENA_LEN: usize = 64 * 1024;

#[repr(C, align(8))]
pub struct Arena(pub [u8; ARENA_LEN]);

/// La RAM que usaba el cargador de arranque: no se inicializa sola.
#[ram(reclaimed)]
static mut ARENA: MaybeUninit<Arena> = MaybeUninit::uninit();
static ARENA_TAKEN: AtomicBool = AtomicBool::new(false);

/// El renderer de la casa.
pub fn renderer() -> &'static mut R {
    RENDERER.take()
}

/// La memoria del [`Arena`], sin inicializar. Una sola vez.
pub fn arena() -> *mut Arena {
    // Sin atómicos de lectura y escritura en este chip: alcanza con una
    // sección crítica.
    let taken = critical_section::with(|_| {
        let t = ARENA_TAKEN.load(Ordering::Relaxed);
        ARENA_TAKEN.store(true, Ordering::Relaxed);
        t
    });
    assert!(!taken, "arena tomada dos veces");
    // SAFETY: la marca garantiza que nadie más la usa.
    unsafe { (*addr_of_mut!(ARENA)).as_mut_ptr() }
}

/// Modos con WiFi: el renderer y el arena pasan a la memoria dinámica.
/// Devuelve cuántos bytes quedaron.
pub fn give_to_heap() -> usize {
    use esp_alloc::{HEAP, HeapRegion, MemoryCapability};
    let r = renderer() as *mut R as *mut u8;
    let a = arena() as *mut u8;
    // SAFETY: las dos zonas son estáticas y, tomadas con `renderer` y
    // `arena`, nadie más las usa en este arranque.
    unsafe {
        HEAP.add_region(HeapRegion::new(
            a,
            ARENA_LEN,
            MemoryCapability::Internal.into(),
        ));
        HEAP.add_region(HeapRegion::new(
            r,
            size_of::<R>(),
            MemoryCapability::Internal.into(),
        ));
    }
    HEAP.free()
}

// --- Pila ----------------------------------------------------------------------

unsafe extern "C" {
    static _stack_end_cpu0: u32;
}

const PAINT: u32 = 0x5EED_CAFE;
/// Lo que se deja sin pintar al fondo: ahí está la guarda contra desbordes.
const GUARD: usize = 256;

fn bottom() -> usize {
    // Solo se toma la dirección del símbolo del linker.
    core::ptr::addr_of!(_stack_end_cpu0) as usize + GUARD
}

/// Pinta la pila libre para después medir cuánto se usó ([`stack_free`]).
pub fn paint_stack() {
    let sp: usize;
    // SAFETY: solo lee el registro.
    unsafe { core::arch::asm!("mv {}, sp", out(reg) sp) };
    let top = (sp - 512) & !3;
    let mut p = bottom();
    while p < top {
        // SAFETY: memoria de la pila por debajo de lo que se está usando.
        unsafe { core::ptr::write_volatile(p as *mut u32, PAINT) };
        p += 4;
    }
}

/// Llama a `f(arg)` con la pila en `[bottom, top)` en vez de la principal,
/// y devuelve cuántos bytes de esa pila usó.
///
/// # Safety
///
/// La zona tiene que ser memoria libre, que nadie use mientras dure la
/// llamada, y alcanzar para `f` y las interrupciones que lleguen (si no,
/// escribe por debajo de `bottom`).
pub unsafe fn call_on_stack(
    bottom: *mut u8,
    top: *mut u8,
    f: unsafe extern "C" fn(*mut u8),
    arg: *mut u8,
) -> usize {
    let lo = (bottom as usize + 3) & !3;
    // La convención de llamadas de RISC-V pide la pila alineada a 16.
    let hi = top as usize & !15;
    let mut p = lo;
    while p < hi {
        // SAFETY: la zona es libre (lo garantiza quien llama).
        unsafe { core::ptr::write_volatile(p as *mut u32, PAINT) };
        p += 4;
    }
    // SAFETY: s2 es de las que `f` tiene que conservar, así que guarda la
    // pila principal durante la llamada; `clobber_abi` cubre el resto.
    unsafe {
        core::arch::asm!(
            "mv s2, sp",
            "mv sp, {top}",
            "jalr ra, 0({f})",
            "mv sp, s2",
            top = in(reg) hi,
            f = in(reg) f,
            in("a0") arg,
            out("s2") _,
            clobber_abi("C"),
        );
    }
    let mut p = lo;
    // SAFETY: como arriba.
    while p < hi && unsafe { core::ptr::read_volatile(p as *const u32) } == PAINT {
        p += 4;
    }
    hi - p
}

/// Bytes de pila que nunca se usaron desde [`paint_stack`].
pub fn stack_free() -> usize {
    let mut p = bottom();
    // SAFETY: lectura de la pila por debajo de lo usado.
    while unsafe { core::ptr::read_volatile(p as *const u32) } == PAINT {
        p += 4;
    }
    p - bottom()
}
