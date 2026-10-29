//! WASAPI clients. Every source is opened in shared mode with AUTOCONVERTPCM, so Windows hands us
//! float32 stereo at our sample rate whatever the device runs at.

use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, Sender};
use serde::Serialize;
use windows::{
	core::{implement, Interface, HRESULT, PCWSTR},
	Win32::{
		Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
		Media::Audio::*,
		System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ},
	},
};

pub const REFTIMES_200MS: i64 = 2_000_000;

pub fn float_format(rate: u32) -> WAVEFORMATEX {
	WAVEFORMATEX {
		wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
		nChannels: 2,
		nSamplesPerSec: rate,
		nAvgBytesPerSec: rate * 8,
		nBlockAlign: 8,
		wBitsPerSample: 32,
		cbSize: 0,
	}
}

const SHARED_FLAGS: u32 = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Completion(Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
	fn ActivateCompleted(&self, _op: windows_core::Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> windows_core::Result<()> {
		let _ = self.0.send(());
		Ok(())
	}
}

/// PROPVARIANT holding a VT_BLOB, laid out by hand (the activation API wants a raw pointer).
#[repr(C)]
struct BlobVariant {
	vt: u16,
	r1: u16,
	r2: u16,
	r3: u16,
	cb: u32,
	p: *const u8,
}

/// Process loopback: captures one process tree (`include`) or everything except one tree.
/// The system mix uses "exclude our own PID", which keeps the UI's save chime out of every clip.
pub fn process_loopback(pid: u32, include: bool, rate: u32) -> Result<IAudioClient> {
	unsafe {
		let params = AUDIOCLIENT_ACTIVATION_PARAMS {
			ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
			Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
				ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: if include { PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE } else { PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE } },
			},
		};
		let blob = BlobVariant { vt: 65, r1: 0, r2: 0, r3: 0, cb: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, p: &params as *const _ as *const u8 };
		let (tx, rx) = bounded(1);
		let handler: IActivateAudioInterfaceCompletionHandler = Completion(tx).into();
		let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&blob as *const _ as *const _), &handler)?;
		rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| anyhow!("audio activation timed out"))?;
		let mut hr = HRESULT(0);
		let mut unk = None;
		op.GetActivateResult(&mut hr, &mut unk)?;
		hr.ok()?;
		let client: IAudioClient = unk.ok_or_else(|| anyhow!("no audio client"))?.cast()?;
		let fmt = float_format(rate);
		client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK | SHARED_FLAGS, REFTIMES_200MS, 0, &fmt, None)?;
		Ok(client)
	}
}

fn enumerator() -> Result<IMMDeviceEnumerator> {
	Ok(unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? })
}

pub fn mic_client(device_id: Option<&str>, rate: u32) -> Result<IAudioClient> {
	mic_client_with(device_id, rate, SHARED_FLAGS)
}

/// A mic client read by polling instead of an event (for the level meter).
pub fn mic_client_polled(device_id: Option<&str>, rate: u32) -> Result<IAudioClient> {
	mic_client_with(device_id, rate, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY)
}

fn mic_client_with(device_id: Option<&str>, rate: u32, flags: u32) -> Result<IAudioClient> {
	unsafe {
		let en = enumerator()?;
		let dev = match device_id {
			Some(id) => {
				let w: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
				en.GetDevice(PCWSTR(w.as_ptr()))?
			}
			None => en.GetDefaultAudioEndpoint(eCapture, eConsole)?,
		};
		let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
		let fmt = float_format(rate);
		client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, REFTIMES_200MS, 0, &fmt, None)?;
		Ok(client)
	}
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
	pub id: String,
	pub name: String,
	pub is_default: bool,
}

pub fn list_mics() -> Result<Vec<AudioDevice>> {
	unsafe {
		let en = enumerator()?;
		let default_id = en.GetDefaultAudioEndpoint(eCapture, eConsole).ok().and_then(|d| d.GetId().ok()).map(|p| {
			let s = p.to_string().unwrap_or_default();
			CoTaskMemFree(Some(p.0 as _));
			s
		});
		let col = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
		let mut out = Vec::new();
		for i in 0..col.GetCount()? {
			let dev = col.Item(i)?;
			let pid = dev.GetId()?;
			let id = pid.to_string().unwrap_or_default();
			CoTaskMemFree(Some(pid.0 as _));
			let name = dev.OpenPropertyStore(STGM_READ).and_then(|s| s.GetValue(&PKEY_Device_FriendlyName)).map(|v| v.to_string()).unwrap_or_else(|_| id.clone());
			out.push(AudioDevice { is_default: default_id.as_deref() == Some(&id), id, name });
		}
		Ok(out)
	}
}

/// A shared-mode client on the default output, fed by polling (for Listen).
pub fn render_client(rate: u32) -> Result<IAudioClient> {
	unsafe {
		let dev = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
		let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
		let fmt = float_format(rate);
		client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, 500_000, 0, &fmt, None)?;
		Ok(client)
	}
}

pub fn default_render_sessions() -> Result<IAudioSessionManager2> {
	unsafe {
		let dev = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
		Ok(dev.Activate(CLSCTX_ALL, None)?)
	}
}
