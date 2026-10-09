//! Optional local Vosk recognition and Windows SAPI speech; no Python or cloud audio.
use anyhow::Result;
#[cfg(not(windows))]
use anyhow::bail;
use std::sync::mpsc::SyncSender;
#[cfg(windows)]
use std::sync::mpsc::sync_channel;
pub struct Speaker {
    sender: SyncSender<String>,
}
impl Speaker {
    pub fn new() -> Result<Self> {
        #[cfg(not(windows))]
        {
            bail!("Voice output requires Windows SAPI");
        }
        #[cfg(windows)]
        {
            use windows::Win32::{
                Media::Speech::{ISpVoice, SPF_DEFAULT, SpVoice},
                System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            };
            let (sender, receiver) = sync_channel::<String>(64);
            let (ready_tx, ready_rx) = sync_channel(1);
            std::thread::spawn(move || {
                let voice = (|| -> Result<ISpVoice> {
                    unsafe {
                        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
                        Ok(CoCreateInstance(&SpVoice, None, CLSCTX_ALL)?)
                    }
                })();
                match voice {
                    Ok(voice) => {
                        let _ = ready_tx.send(Ok(()));
                        while let Ok(text) = receiver.recv() {
                            let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
                            let _ = unsafe {
                                voice.Speak(
                                    windows::core::PCWSTR(wide.as_ptr()),
                                    SPF_DEFAULT.0 as u32,
                                    None,
                                )
                            };
                        }
                        drop(voice);
                        unsafe {
                            windows::Win32::System::Com::CoUninitialize();
                        }
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            });
            ready_rx.recv()?.map_err(anyhow::Error::msg)?;
            Ok(Self { sender })
        }
    }
    pub fn speak(&self, text: &str) {
        let _ = self.sender.try_send(text.into());
    }
}
pub fn listen(model: &std::path::Path) -> Result<String> {
    #[cfg(not(windows))]
    {
        let _ = model;
        bail!("Microphone/Vosk mode requires Windows");
    }
    #[cfg(windows)]
    {
        native::listen(model)
    }
}
#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::{CString, c_char, c_void},
        mem::size_of,
        path::Path,
    };
    #[repr(C)]
    struct WaveFormat {
        tag: u16,
        channels: u16,
        samples: u32,
        bytes: u32,
        align: u16,
        bits: u16,
        extra: u16,
    }
    #[repr(C)]
    struct WaveHeader {
        data: *mut i8,
        length: u32,
        recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: *mut c_void,
        reserved: usize,
    }
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn waveInOpen(
            handle: *mut *mut c_void,
            device: u32,
            format: *const WaveFormat,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        fn waveInPrepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveInAddBuffer(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveInStart(handle: *mut c_void) -> u32;
        fn waveInReset(handle: *mut c_void) -> u32;
        fn waveInUnprepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
        fn waveInClose(handle: *mut c_void) -> u32;
    }
    fn check(code: u32) -> Result<()> {
        anyhow::ensure!(code == 0, "Microphone error {code}");
        Ok(())
    }
    fn record() -> Result<Vec<u8>> {
        let mut samples = vec![0u8; 16000 * 6 * 2];
        let format = WaveFormat {
            tag: 1,
            channels: 1,
            samples: 16000,
            bytes: 32000,
            align: 2,
            bits: 16,
            extra: 0,
        };
        let mut handle = std::ptr::null_mut();
        let mut header = WaveHeader {
            data: samples.as_mut_ptr().cast(),
            length: samples.len() as u32,
            recorded: 0,
            user: 0,
            flags: 0,
            loops: 0,
            next: std::ptr::null_mut(),
            reserved: 0,
        };
        // SAFETY: buffers and headers remain allocated until reset/unprepare/close;
        // recording is mono PCM16 and all multimedia return codes are checked.
        unsafe {
            check(waveInOpen(&mut handle, u32::MAX, &format, 0, 0, 0))?;
            let mut prepared = false;
            let result: Result<()> = (|| {
                check(waveInPrepareHeader(
                    handle,
                    &mut header,
                    size_of::<WaveHeader>() as u32,
                ))?;
                prepared = true;
                check(waveInAddBuffer(
                    handle,
                    &mut header,
                    size_of::<WaveHeader>() as u32,
                ))?;
                check(waveInStart(handle))?;
                std::thread::sleep(std::time::Duration::from_secs(6));
                Ok(())
            })();
            let reset = waveInReset(handle);
            let unprepare = if prepared {
                waveInUnprepareHeader(handle, &mut header, size_of::<WaveHeader>() as u32)
            } else {
                0
            };
            let close = waveInClose(handle);
            result?;
            check(reset)?;
            check(unprepare)?;
            check(close)?;
        }
        samples.truncate(header.recorded as usize);
        anyhow::ensure!(!samples.is_empty(), "Microphone returned no samples");
        Ok(samples)
    }
    pub fn listen(path: &Path) -> Result<String> {
        anyhow::ensure!(path.is_dir(), "Provide a local 16 kHz Vosk model directory");
        let dll = std::env::var_os("VOSK_DLL")
            .map(std::path::PathBuf::from)
            .unwrap_or(
                std::env::current_exe()?
                    .parent()
                    .unwrap()
                    .join("libvosk.dll"),
            );
        // SAFETY: opaque Vosk handles are only passed to the corresponding API;
        // every successful allocation is freed before the library is unloaded.
        unsafe {
            let library=libloading::Library::new(dll).map_err(|_|anyhow::anyhow!("Install the official native Vosk libvosk.dll and companion DLLs next to the executable, or set VOSK_DLL"))?;
            let model_new: unsafe extern "C" fn(*const c_char) -> *mut c_void =
                *library.get(b"vosk_model_new\0")?;
            let model_free: unsafe extern "C" fn(*mut c_void) =
                *library.get(b"vosk_model_free\0")?;
            let rec_new: unsafe extern "C" fn(*mut c_void, f32) -> *mut c_void =
                *library.get(b"vosk_recognizer_new\0")?;
            let rec_free: unsafe extern "C" fn(*mut c_void) =
                *library.get(b"vosk_recognizer_free\0")?;
            let accept: unsafe extern "C" fn(*mut c_void, *const c_char, i32) -> i32 =
                *library.get(b"vosk_recognizer_accept_waveform\0")?;
            let final_result: unsafe extern "C" fn(*mut c_void) -> *const c_char =
                *library.get(b"vosk_recognizer_final_result\0")?;
            let name = CString::new(path.to_string_lossy().as_bytes())?;
            let model = model_new(name.as_ptr());
            anyhow::ensure!(!model.is_null(), "Vosk failed to load model");
            let rec = rec_new(model, 16000.);
            if rec.is_null() {
                model_free(model);
                anyhow::bail!("Vosk recognizer failed");
            }
            let result = (|| {
                let samples = record()?;
                anyhow::ensure!(
                    accept(rec, samples.as_ptr().cast(), samples.len() as i32) >= 0,
                    "Vosk rejected samples"
                );
                let ptr = final_result(rec);
                anyhow::ensure!(!ptr.is_null(), "Vosk returned null result");
                let text = std::ffi::CStr::from_ptr(ptr).to_str()?;
                let value: serde_json::Value = serde_json::from_str(text)?;
                Ok(value["text"].as_str().unwrap_or("").trim().to_owned())
            })();
            rec_free(rec);
            model_free(model);
            result
        }
    }
}
