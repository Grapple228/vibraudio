#[derive(Clone, Copy)]
pub enum Priority {
    Critical,
    Highest,
    AboveNormal,
    Normal,
}

impl Priority {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Priority::Critical => "critical",
            Priority::Highest => "highest",
            Priority::AboveNormal => "above normal",
            Priority::Normal => "normal",
        }
    }

    #[cfg(target_os = "windows")]
    pub const fn as_value(&self) -> i32 {
        use winapi::um::winbase::{
            THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_NORMAL,
            THREAD_PRIORITY_TIME_CRITICAL,
        };

        match self {
            Priority::Critical => THREAD_PRIORITY_TIME_CRITICAL as i32,
            Priority::Highest => THREAD_PRIORITY_HIGHEST as i32,
            Priority::AboveNormal => THREAD_PRIORITY_ABOVE_NORMAL as i32,
            Priority::Normal => THREAD_PRIORITY_NORMAL as i32,
        }
    }
}

#[cfg(target_os = "windows")]
pub enum MmcssValue {
    Audio,
    Capture,
    Distribution,
    Games,
    Playback,
    ProAudio,
    WindowsManager,
    None,
}

#[cfg(target_os = "windows")]
impl MmcssValue {
    pub const fn as_str(&self) -> &'static str {
        match self {
            MmcssValue::Audio => "Audio",
            MmcssValue::Capture => "Capture",
            MmcssValue::Distribution => "Distribution",
            MmcssValue::Games => "Games",
            MmcssValue::Playback => "Playback",
            MmcssValue::ProAudio => "Pro Audio",
            MmcssValue::WindowsManager => "Windows Manager",
            MmcssValue::None => "None",
        }
    }

    pub const fn needs_enable(&self) -> bool {
        !matches!(self, MmcssValue::None)
    }
}

#[cfg(target_os = "windows")]
type Return = Option<MmcssHandle>;
#[cfg(not(target_os = "windows"))]
type Return = ();

pub fn configure_audio_thread(
    priority: Priority,
    #[cfg(target_os = "windows")] mmcss_value: MmcssValue,
) -> Return {
    set_thread_priority(priority);

    #[cfg(target_os = "windows")]
    {
        return enable_mmcss(mmcss_value);
    }

    #[cfg(not(target_os = "windows"))]
    ()
}

#[cfg(target_os = "windows")]
pub struct MmcssHandle(winapi::um::winnt::HANDLE, #[allow(unused)] u32);

#[cfg(target_os = "windows")]
impl Drop for MmcssHandle {
    fn drop(&mut self) {
        unsafe {
            use winapi::um::avrt::AvRevertMmThreadCharacteristics;
            AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn enable_mmcss(mmcss_value: MmcssValue) -> Option<MmcssHandle> {
    use winapi::um::avrt::AvSetMmThreadCharacteristicsW;

    if !mmcss_value.needs_enable() {
        return None;
    }

    let task_name = mmcss_value.as_str();

    // encode utf-16
    let mut wide_buf = [0u16; 32];

    let mut i = 0;
    for ch in task_name.encode_utf16() {
        if i < wide_buf.len() - 1 {
            wide_buf[i] = ch;
            i += 1;
        }
    }

    let mut task_index: u32 = 0;
    let handle = unsafe { AvSetMmThreadCharacteristicsW(wide_buf.as_ptr(), &mut task_index) };

    if handle.is_null() {
        let err = std::io::Error::last_os_error();
        eprintln!("Failed: {:?}", err);
        None
    } else {
        tracing::debug!(
            "✅ MMCSS successfully set for '{}' (task_index: {}, handle: {:?})",
            task_name,
            task_index,
            handle
        );

        Some(MmcssHandle(handle, task_index))
    }
}

pub fn set_thread_priority(priority: Priority) {
    #[cfg(target_os = "windows")]
    unsafe {
        use winapi::um::processthreadsapi::{GetCurrentThread, SetThreadPriority};

        let handle = GetCurrentThread();
        let result = SetThreadPriority(handle, priority.as_value());

        if result == 0 {
            eprintln!(
                "Failed to set thread priority: {}",
                std::io::Error::last_os_error()
            );
        } else {
            tracing::debug!("✅ Thread priority set to: {}", priority.as_str());
        }
    }
}
