#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

use std::ffi::c_void;

// COM interfaces
#[repr(C)]
pub struct IUnknown {
    vtable: *const c_void,
}

#[repr(C)]
pub struct IAudioClient {
    vtable: *const c_void,
}

#[repr(C)]
pub struct IAudioRenderClient {
    vtable: *const c_void,
}

#[repr(C)]
pub struct IAudioCaptureClient {
    vtable: *const c_void,
}

#[repr(C)]
pub struct IMMDevice {
    vtable: *const c_void,
}

#[repr(C)]
pub struct IMMDeviceEnumerator {
    vtable: *const c_void,
}

// Constants
pub const AUDCLNT_SHAREMODE_SHARED: u32 = 0;
pub const AUDCLNT_SHAREMODE_EXCLUSIVE: u32 = 1;
pub const AUDCLNT_STREAMFLAGS_EVENTCALLBACK: u32 = 0x00040000;
pub const CLSCTX_INPROC_SERVER: u32 = 1;

// GUID structure
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

// CLSID_MMDeviceEnumerator: {BCDE0395-E52F-467C-8E3D-C4579291692E}
pub const CLSID_MMDeviceEnumerator: GUID = GUID {
    data1: 0xBCDE0395,
    data2: 0xE52F,
    data3: 0x467C,
    data4: [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
};

// IID_IMMDeviceEnumerator: {A95664D2-9614-4F35-A746-DE8DB63617E6}
pub const IID_IMMDeviceEnumerator: GUID = GUID {
    data1: 0xA95664D2,
    data2: 0x9614,
    data3: 0x4F35,
    data4: [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
};

// IID_IAudioClient: {1CB9AD4C-DBFA-4C32-B178-C2F568A703B2}
pub const IID_IAudioClient: GUID = GUID {
    data1: 0x1CB9AD4C,
    data2: 0xDBFA,
    data3: 0x4C32,
    data4: [0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2],
};

// IID_IAudioRenderClient: {F294ACFC-3146-4483-A7BF-ADDCA7C260E2}
pub const IID_IAudioRenderClient: GUID = GUID {
    data1: 0xF294ACFC,
    data2: 0x3146,
    data3: 0x4483,
    data4: [0xA7, 0xBF, 0xAD, 0xDC, 0xA7, 0xC2, 0x60, 0xE2],
};

// IID_IAudioCaptureClient: {C8ADBD64-E71E-48A0-A4DE-185C395CD317}
pub const IID_IAudioCaptureClient: GUID = GUID {
    data1: 0xC8ADBD64,
    data2: 0xE71E,
    data3: 0x48A0,
    data4: [0xA4, 0xDE, 0x18, 0x5C, 0x39, 0x5C, 0xD3, 0x17],
};

// IID_IUnknown: {00000000-0000-0000-C000-000000000046}
pub const IID_IUnknown: GUID = GUID {
    data1: 0x00000000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum EDataFlow {
    eRender = 0,
    eCapture = 1,
    eAll = 2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum ERole {
    eConsole = 0,
    eMultimedia = 1,
    eCommunications = 2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct WAVEFORMATEX {
    pub wFormatTag: u16,
    pub nChannels: u16,
    pub nSamplesPerSec: u32,
    pub nAvgBytesPerSec: u32,
    pub nBlockAlign: u16,
    pub wBitsPerSample: u16,
    pub cbSize: u16,
}

// COM function types
pub type IUnknownQueryInterface = unsafe extern "system" fn(
    this: *mut c_void,
    riid: *const GUID,
    ppvObject: *mut *mut c_void,
) -> i32;

pub type IUnknownAddRef = unsafe extern "system" fn(this: *mut c_void) -> u32;
pub type IUnknownRelease = unsafe extern "system" fn(this: *mut c_void) -> u32;

// Helper to get vtable function
#[inline]
unsafe fn vtable_call<F>(ptr: *mut c_void, index: usize) -> F {
    let vtable = *(ptr as *const *const *const c_void);
    let func_ptr: *const c_void = *vtable.add(index);
    std::mem::transmute_copy(&func_ptr)
}

// IUnknown methods
#[inline]
pub unsafe fn com_query_interface(ptr: *mut c_void, riid: &GUID, ppv: *mut *mut c_void) -> i32 {
    let func: IUnknownQueryInterface = vtable_call(ptr, 0);
    func(ptr, riid, ppv)
}

#[inline]
pub unsafe fn com_add_ref(ptr: *mut c_void) -> u32 {
    let func: IUnknownAddRef = vtable_call(ptr, 1);
    func(ptr)
}

#[inline]
pub unsafe fn com_release(ptr: *mut c_void) -> u32 {
    let func: IUnknownRelease = vtable_call(ptr, 2);
    func(ptr)
}

// COM API functions
#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: u32) -> i32;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(
        rclsid: *const GUID,
        pUnkOuter: *mut c_void,
        dwClsContext: u32,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> i32;
    pub fn CoTaskMemFree(pv: *mut c_void);
}

// IMMDeviceEnumerator methods
#[inline]
pub unsafe fn IMMDeviceEnumerator_GetDefaultAudioEndpoint(
    enumerator: *mut IMMDeviceEnumerator,
    data_flow: EDataFlow,
    role: ERole,
    device: *mut *mut IMMDevice,
) -> i32 {
    type Func =
        unsafe extern "system" fn(*mut c_void, EDataFlow, ERole, *mut *mut IMMDevice) -> i32;
    let func: Func = vtable_call(enumerator as *mut c_void, 4);
    func(enumerator as *mut c_void, data_flow, role, device)
}

#[inline]
pub unsafe fn IMMDeviceEnumerator_EnumAudioEndpoints(
    enumerator: *mut IMMDeviceEnumerator,
    data_flow: EDataFlow,
    state_mask: u32,
    devices: *mut *mut c_void,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, EDataFlow, u32, *mut *mut c_void) -> i32;
    let func: Func = vtable_call(enumerator as *mut c_void, 3);
    func(enumerator as *mut c_void, data_flow, state_mask, devices)
}

// IMMDevice methods
#[inline]
pub unsafe fn IMMDevice_Activate(
    device: *mut IMMDevice,
    iid: *const GUID,
    dw_cls_ctx: u32,
    activation_params: *mut c_void,
    interface_ptr: *mut *mut c_void,
) -> i32 {
    type Func = unsafe extern "system" fn(
        *mut c_void,
        *const GUID,
        u32,
        *mut c_void,
        *mut *mut c_void,
    ) -> i32;
    let func: Func = vtable_call(device as *mut c_void, 3);
    func(
        device as *mut c_void,
        iid,
        dw_cls_ctx,
        activation_params,
        interface_ptr,
    )
}

#[inline]
pub unsafe fn IMMDevice_GetId(device: *mut IMMDevice, id: *mut *mut u16) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32;
    let func: Func = vtable_call(device as *mut c_void, 4);
    func(device as *mut c_void, id)
}

// IAudioClient methods
#[inline]
pub unsafe fn IAudioClient_Initialize(
    client: *mut IAudioClient,
    share_mode: u32,
    stream_flags: u32,
    buffer_duration: u64,
    periodicity: u64,
    format: *const WAVEFORMATEX,
    audio_session_guid: *const GUID,
) -> i32 {
    type Func = unsafe extern "system" fn(
        *mut c_void,
        u32,
        u32,
        u64,
        u64,
        *const WAVEFORMATEX,
        *const GUID,
    ) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 3);
    func(
        client as *mut c_void,
        share_mode,
        stream_flags,
        buffer_duration,
        periodicity,
        format,
        audio_session_guid,
    )
}

#[inline]
pub unsafe fn IAudioClient_GetBufferSize(client: *mut IAudioClient, buffer_size: *mut u32) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, *mut u32) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 4);
    func(client as *mut c_void, buffer_size)
}

#[inline]
pub unsafe fn IAudioClient_GetCurrentPadding(
    client: *mut IAudioClient,
    num_padding_frames: *mut u32,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, *mut u32) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 6);
    func(client as *mut c_void, num_padding_frames)
}

#[inline]
pub unsafe fn IAudioClient_IsFormatSupported(
    client: *mut IAudioClient,
    share_mode: u32,
    format: *const WAVEFORMATEX,
    closest_match: *mut *mut WAVEFORMATEX,
) -> i32 {
    type Func = unsafe extern "system" fn(
        *mut c_void,
        u32,
        *const WAVEFORMATEX,
        *mut *mut WAVEFORMATEX,
    ) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 7);
    func(client as *mut c_void, share_mode, format, closest_match)
}

#[inline]
pub unsafe fn IAudioClient_GetMixFormat(
    client: *mut IAudioClient,
    pp_format: *mut *mut WAVEFORMATEX,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, *mut *mut WAVEFORMATEX) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 8);
    func(client as *mut c_void, pp_format)
}

#[inline]
pub unsafe fn IAudioClient_GetService(
    client: *mut IAudioClient,
    riid: *const GUID,
    service: *mut *mut c_void,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 14);
    func(client as *mut c_void, riid, service)
}

#[inline]
pub unsafe fn IAudioClient_Start(client: *mut IAudioClient) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 10);
    func(client as *mut c_void)
}

#[inline]
pub unsafe fn IAudioClient_Stop(client: *mut IAudioClient) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 11);
    func(client as *mut c_void)
}

#[inline]
pub unsafe fn IAudioClient_Reset(client: *mut IAudioClient) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 12);
    func(client as *mut c_void)
}

// IAudioRenderClient methods
#[inline]
pub unsafe fn IAudioRenderClient_GetBuffer(
    client: *mut IAudioRenderClient,
    num_frames_requested: u32,
    data: *mut *mut u8,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, u32, *mut *mut u8) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 3);
    func(client as *mut c_void, num_frames_requested, data)
}

#[inline]
pub unsafe fn IAudioRenderClient_ReleaseBuffer(
    client: *mut IAudioRenderClient,
    num_frames_written: u32,
    flags: u32,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, u32, u32) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 4);
    func(client as *mut c_void, num_frames_written, flags)
}

// IAudioCaptureClient methods
#[inline]
pub unsafe fn IAudioCaptureClient_GetBuffer(
    client: *mut IAudioCaptureClient,
    data: *mut *mut u8,
    num_frames_to_read: *mut u32,
    flags: *mut u32,
    device_position: *mut u64,
    qpc_position: *mut u64,
) -> i32 {
    type Func = unsafe extern "system" fn(
        *mut c_void,
        *mut *mut u8,
        *mut u32,
        *mut u32,
        *mut u64,
        *mut u64,
    ) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 3);
    func(
        client as *mut c_void,
        data,
        num_frames_to_read,
        flags,
        device_position,
        qpc_position,
    )
}

#[inline]
pub unsafe fn IAudioCaptureClient_ReleaseBuffer(
    client: *mut IAudioCaptureClient,
    num_frames_read: u32,
) -> i32 {
    type Func = unsafe extern "system" fn(*mut c_void, u32) -> i32;
    let func: Func = vtable_call(client as *mut c_void, 4);
    func(client as *mut c_void, num_frames_read)
}
