//! Dynamically loaded official SimConnect ABI. No SDK binaries are bundled.
use super::*;
use anyhow::{Context, ensure};
use libloading::Library;
use std::{
    ffi::{CString, c_void},
    mem::size_of,
};
type Handle = *mut c_void;
type Open = unsafe extern "system" fn(*mut Handle, *const i8, Handle, u32, Handle, u32) -> i32;
type Close = unsafe extern "system" fn(Handle) -> i32;
type Create = unsafe extern "system" fn(Handle, *const i8, *const i8, InitPosition, u32) -> i32;
type Remove = unsafe extern "system" fn(Handle, u32, u32) -> i32;
type Add = unsafe extern "system" fn(Handle, u32, *const i8, *const i8, u32, f32, u32) -> i32;
type Set = unsafe extern "system" fn(Handle, u32, u32, u32, u32, u32, *const c_void) -> i32;
type Request = unsafe extern "system" fn(Handle, u32, u32, u32, u32, u32, u32, u32, u32) -> i32;
type Next = unsafe extern "system" fn(Handle, *mut *const Recv, *mut u32) -> i32;
pub struct SimConnect {
    _library: Library,
    handle: Handle,
    create_fn: Create,
    remove_fn: Remove,
    set_fn: Set,
    next_fn: Next,
    close_fn: Close,
    allow_motion: bool,
    connected: bool,
    request: u32,
    pending: BTreeMap<u32, Option<String>>,
    positions: BTreeMap<String, Position>,
    objects: BTreeMap<String, u32>,
    player: Option<Position>,
    radio: Option<CockpitRadio>,
}
fn check(result: i32, op: &str) -> Result<()> {
    ensure!(result >= 0, "SimConnect {op} HRESULT {result:#010x}");
    Ok(())
}
impl SimConnect {
    pub fn connect(allow_motion: bool) -> Result<Self> {
        // SAFETY: function pointers remain valid while the library is retained;
        // all signatures follow the 64-bit SimConnect SDK C ABI.
        unsafe {
            let library = Library::new("SimConnect.dll")
                .context("SimConnect.dll not found; install MSFS runtime/SDK")?;
            let open: Open = *library.get(b"SimConnect_Open\0")?;
            let close: Close = *library.get(b"SimConnect_Close\0")?;
            let create: Create = *library.get(b"SimConnect_AICreateNonATCAircraft\0")?;
            let remove: Remove = *library.get(b"SimConnect_AIRemoveObject\0")?;
            let add: Add = *library.get(b"SimConnect_AddToDataDefinition\0")?;
            let set: Set = *library.get(b"SimConnect_SetDataOnSimObject\0")?;
            let request: Request = *library.get(b"SimConnect_RequestDataOnSimObject\0")?;
            let next: Next = *library.get(b"SimConnect_GetNextDispatch\0")?;
            let mut handle = std::ptr::null_mut();
            check(
                open(
                    &mut handle,
                    c"RealFlow Rust".as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    0,
                ),
                "Open",
            )?;
            let mut bridge = Self {
                _library: library,
                handle,
                create_fn: create,
                remove_fn: remove,
                set_fn: set,
                next_fn: next,
                close_fn: close,
                allow_motion,
                connected: true,
                request: 1000,
                pending: BTreeMap::new(),
                positions: BTreeMap::new(),
                objects: BTreeMap::new(),
                player: None,
                radio: None,
            };
            check(
                add(
                    handle,
                    1,
                    c"Initial Position".as_ptr(),
                    std::ptr::null(),
                    12,
                    0.,
                    u32::MAX,
                ),
                "Define position",
            )?;
            for (name, unit) in [
                ("PLANE LATITUDE", "degrees"),
                ("PLANE LONGITUDE", "degrees"),
                ("PLANE ALTITUDE", "feet"),
                ("SIM ON GROUND", "Bool"),
            ] {
                let n = CString::new(name)?;
                let u = CString::new(unit)?;
                check(
                    add(handle, 2, n.as_ptr(), u.as_ptr(), 4, 0., u32::MAX),
                    "Define player",
                )?;
            }
            check(request(handle, 100, 2, 0, 4, 0, 0, 0, 0), "Request player")?;
            for (name, unit) in [
                ("COM ACTIVE FREQUENCY:1", "MHz"),
                ("COM ACTIVE FREQUENCY:2", "MHz"),
                ("COM TRANSMIT:1", "Bool"),
                ("COM TRANSMIT:2", "Bool"),
                ("TRANSPONDER CODE:1", "Number"),
            ] {
                let n = CString::new(name)?;
                let u = CString::new(unit)?;
                check(
                    add(handle, 3, n.as_ptr(), u.as_ptr(), 4, 0., u32::MAX),
                    "Define radio",
                )?;
            }
            check(request(handle, 101, 3, 0, 4, 0, 0, 0, 0), "Request radio")?;
            bridge.poll()?;
            Ok(bridge)
        }
    }
    fn req(&mut self) -> u32 {
        self.request += 1;
        self.request
    }
}
impl Bridge for SimConnect {
    fn poll(&mut self) -> Result<()> {
        if !self.connected {
            bail!("Simulator disconnected");
        }
        for _ in 0..256 {
            let mut ptr = std::ptr::null();
            let mut bytes = 0;
            // SAFETY: the dispatch buffer belongs to SimConnect until the next call.
            // Every field read below is bounded by its declared and returned size.
            if unsafe { (self.next_fn)(self.handle, &mut ptr, &mut bytes) } < 0 {
                break;
            }
            if ptr.is_null() || bytes < size_of::<Recv>() as u32 {
                continue;
            }
            let hdr = unsafe { std::ptr::read_unaligned(ptr) };
            let size = bytes.min(hdr.size) as usize;
            match hdr.id {
                12 if size >= size_of::<Assigned>() => {
                    let a = unsafe { std::ptr::read_unaligned(ptr.cast::<Assigned>()) };
                    match self.pending.remove(&a.request) {
                        Some(Some(key)) => {
                            self.objects.insert(key.clone(), a.object);
                            if let Some(p) = self.positions.get(&key).copied() {
                                self.update(&key, p)?;
                            }
                        }
                        Some(None) => {
                            let req = self.req();
                            check(
                                unsafe { (self.remove_fn)(self.handle, a.object, req) },
                                "Remove orphan",
                            )?;
                        }
                        None => {}
                    }
                }
                8 if size >= 40 => {
                    let raw = ptr.cast::<u8>();
                    let request = unsafe { std::ptr::read_unaligned(raw.add(12).cast::<u32>()) };
                    let read = |index: usize| unsafe {
                        std::ptr::read_unaligned(raw.add(40 + index * 8).cast::<f64>())
                    };
                    if request == 100 && size >= 72 {
                        let p = Position {
                            on_ground: read(3) > 0.5,
                            ..Position::new(read(0), read(1), read(2))
                        };
                        if p.validate().is_ok() {
                            self.player = Some(p);
                        }
                    }
                    if request == 101 && size >= 80 {
                        self.radio = Some(CockpitRadio {
                            com1: valid_mhz(read(0)),
                            com2: valid_mhz(read(1)),
                            transmitting: if read(3) > 0.5 && read(2) <= 0.5 {
                                2
                            } else {
                                1
                            },
                            squawk: decode_squawk(read(4)),
                        });
                    }
                }
                3 => {
                    self.connected = false;
                    bail!("Simulator closed");
                }
                1 => bail!("SimConnect reported an exception; inspect simulator SDK output"),
                _ => {}
            }
        }
        Ok(())
    }
    fn create(&mut self, key: &str, title: &str, p: Position) -> Result<()> {
        p.validate()?;
        ensure!(self.connected, "Not connected");
        if self.objects.contains_key(key)
            || self.pending.values().any(|v| v.as_deref() == Some(key))
        {
            return Ok(());
        }
        let title = CString::new(title)?;
        let tail = CString::new(
            key.chars()
                .rev()
                .take(12)
                .collect::<String>()
                .chars()
                .rev()
                .filter(char::is_ascii)
                .collect::<String>(),
        )?;
        let req = self.req();
        check(
            unsafe { (self.create_fn)(self.handle, title.as_ptr(), tail.as_ptr(), p.into(), req) },
            "Create aircraft",
        )?;
        self.pending.insert(req, Some(key.into()));
        self.positions.insert(key.into(), p);
        Ok(())
    }
    fn update(&mut self, key: &str, p: Position) -> Result<()> {
        p.validate()?;
        self.positions.insert(key.into(), p);
        if !self.allow_motion || !self.connected {
            return Ok(());
        }
        if let Some(object) = self.objects.get(key).copied() {
            let p: InitPosition = p.into();
            check(
                unsafe {
                    (self.set_fn)(
                        self.handle,
                        1,
                        object,
                        0,
                        0,
                        size_of::<InitPosition>() as u32,
                        (&p as *const InitPosition).cast(),
                    )
                },
                "Set position",
            )?;
        }
        Ok(())
    }
    fn remove(&mut self, key: &str) -> Result<()> {
        self.positions.remove(key);
        for value in self.pending.values_mut() {
            if value.as_deref() == Some(key) {
                *value = None;
            }
        }
        if let Some(object) = self.objects.remove(key)
            && self.connected
        {
            let req = self.req();
            check(
                unsafe { (self.remove_fn)(self.handle, object, req) },
                "Remove aircraft",
            )?;
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        let keys: Vec<_> = self.objects.keys().cloned().collect();
        let mut error = None;
        for key in keys {
            if let Err(e) = self.remove(&key) {
                error.get_or_insert(e);
            }
        }
        // Process pending creation IDs before closing the connection, so removed
        // requests are cleaned instead of leaving orphan AI objects.
        if self.connected {
            let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while !self.pending.is_empty() && std::time::Instant::now() < until {
                let _ = self.poll();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        if !self.handle.is_null() {
            if let Err(e) = check(unsafe { (self.close_fn)(self.handle) }, "Close") {
                error.get_or_insert(e);
            }
            self.handle = std::ptr::null_mut();
        }
        self.connected = false;
        self.pending.clear();
        self.positions.clear();
        if let Some(e) = error {
            return Err(e);
        }
        Ok(())
    }
    fn player_position(&self) -> Option<Position> {
        self.player
    }
    fn player_radio(&self) -> Option<CockpitRadio> {
        self.radio.clone()
    }
}
impl Drop for SimConnect {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
