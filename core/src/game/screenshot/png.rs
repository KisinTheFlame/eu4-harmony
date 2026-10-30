//! A frame's pixels as a PNG, scaled and encoded with Windows' own imaging
//! (WIC), on the thread of the framework's own that takes a screenshot.
use windows::core::{Interface, Result, GUID};
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, StructuredStorage::CreateStreamOnHGlobal, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STREAM_SEEK_END,
    STREAM_SEEK_SET,
};

/// COM on the current thread, for as long as it lives.
pub struct Com(bool);

impl Com {
    #[expect(clippy::new_without_default, reason = "making one initializes COM on the thread, which a default would hide")]
    pub fn new() -> Self {
        // S_FALSE, already initialized, is balanced by an uninitialize too;
        // a thread in another apartment keeps it, and WIC works there.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// A side of the image at `percent` of the frame's.
pub fn scaled(side: u32, percent: u32) -> u32 {
    ((side as u64 * percent as u64 + 50) / 100).max(1) as u32
}

/// A frame's pixels, held by WIC.
pub struct Picture {
    factory: IWICImagingFactory,
    bitmap: IWICBitmap,
    pub width: u32,
    pub height: u32,
}

impl Picture {
    /// Copies `height` rows of `width` pixels, 4 bytes each (blue, green,
    /// red and one unused, as Direct3D's X8R8G8B8 and A8R8G8B8 keep them),
    /// the rows `pitch` bytes apart from `bits` on. COM must be initialized
    /// on the thread.
    ///
    /// # Safety
    /// `bits` must hold that many rows.
    pub unsafe fn copy(bits: *const u8, pitch: usize, width: u32, height: u32) -> Result<Self> {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let bitmap = factory.CreateBitmap(width, height, &GUID_WICPixelFormat32bppBGR, WICBitmapCacheOnLoad)?;
        {
            let lock = bitmap.Lock(&WICRect { X: 0, Y: 0, Width: width as i32, Height: height as i32 }, WICBitmapLockWrite.0 as u32)?;
            let stride = lock.GetStride()? as usize;
            let (mut size, mut data) = (0u32, std::ptr::null_mut());
            lock.GetDataPointer(&mut size, &mut data)?;
            let row = width as usize * 4;
            for y in 0..height as usize {
                std::ptr::copy_nonoverlapping(bits.add(y * pitch), data.add(y * stride), row);
            }
        }
        Ok(Self { factory, bitmap, width, height })
    }

    /// The picture at `percent` of its size (1 to 100), as a PNG: its bytes,
    /// its width and its height. A smaller picture is the average of the
    /// pixels each of its pixels covers.
    pub fn png(&self, percent: u32) -> Result<(Vec<u8>, u32, u32)> {
        let (width, height) = (scaled(self.width, percent), scaled(self.height, percent));
        unsafe {
            let source: IWICBitmapSource = if (width, height) == (self.width, self.height) {
                self.bitmap.cast()?
            } else {
                let scaler = self.factory.CreateBitmapScaler()?;
                scaler.Initialize(&self.bitmap, width, height, WICBitmapInterpolationModeFant)?;
                scaler.cast()?
            };
            // The unused byte goes: the PNG has no alpha.
            let rgb = self.factory.CreateFormatConverter()?;
            rgb.Initialize(&source, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
            let encoder = self.factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let (mut frame, mut options) = (None, None);
            encoder.CreateNewFrame(&mut frame, &mut options)?;
            let frame = frame.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_POINTER))?;
            frame.Initialize(options.as_ref())?;
            frame.SetSize(width, height)?;
            let mut format: GUID = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut format)?;
            frame.WriteSource(&rgb, std::ptr::null())?;
            frame.Commit()?;
            encoder.Commit()?;
            let mut size = 0u64;
            stream.Seek(0, STREAM_SEEK_END, Some(&mut size))?;
            stream.Seek(0, STREAM_SEEK_SET, None)?;
            let mut bytes = vec![0u8; size as usize];
            let mut done = 0;
            while done < bytes.len() {
                let mut read = 0u32;
                let want = (bytes.len() - done).min(u32::MAX as usize) as u32;
                stream.Read(bytes[done..].as_mut_ptr().cast(), want, Some(&mut read)).ok()?;
                if read == 0 {
                    break;
                }
                done += read as usize;
            }
            bytes.truncate(done);
            Ok((bytes, width, height))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pixels of a PNG, as 24-bit blue, green, red, row after row.
    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        unsafe {
            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let stream = factory.CreateStream().unwrap();
            stream.InitializeFromMemory(png).unwrap();
            let decoder = factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).unwrap();
            assert_eq!(decoder.GetContainerFormat().unwrap(), GUID_ContainerFormatPng);
            let frame = decoder.GetFrame(0).unwrap();
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height).unwrap();
            let rgb = factory.CreateFormatConverter().unwrap();
            rgb.Initialize(&frame, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).unwrap();
            let mut pixels = vec![0u8; (width * height * 3) as usize];
            rgb.CopyPixels(std::ptr::null(), width * 3, &mut pixels).unwrap();
            (width, height, pixels)
        }
    }

    /// Four pixels across, two down, rows padded to 20 bytes as a locked
    /// surface's may be: the left half blue, the right half red, the unused
    /// byte left as junk.
    fn frame() -> Vec<u8> {
        let mut rows = Vec::new();
        for _ in 0..2 {
            rows.extend([255, 0, 0, 7, 255, 0, 0, 7, 0, 0, 255, 9, 0, 0, 255, 9]);
            rows.extend([0xee; 4]);
        }
        rows
    }

    #[test]
    fn a_frame_becomes_a_png_of_its_pixels() {
        let _com = Com::new();
        let rows = frame();
        let picture = unsafe { Picture::copy(rows.as_ptr(), 20, 4, 2) }.unwrap();
        let (png, width, height) = picture.png(100).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!((width, height), (4, 2));
        let blue_red = [255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255];
        assert_eq!(decode(&png), (4, 2, [blue_red, blue_red].concat()));
    }

    #[test]
    fn a_smaller_picture_averages_the_pixels_it_covers() {
        let _com = Com::new();
        let rows = frame();
        let picture = unsafe { Picture::copy(rows.as_ptr(), 20, 4, 2) }.unwrap();
        let (png, width, height) = picture.png(50).unwrap();
        assert_eq!((width, height), (2, 1));
        assert_eq!(decode(&png), (2, 1, vec![255, 0, 0, 0, 0, 255]));
        // Never smaller than a pixel.
        assert_eq!(picture.png(1).unwrap().1, 1);
        assert_eq!((scaled(3840, 50), scaled(2160, 33), scaled(5, 1)), (1920, 713, 1));
    }

    #[test]
    fn a_large_frame_is_encoded() {
        let _com = Com::new();
        let (width, height) = (1920u32, 1080u32);
        let rows: Vec<u8> = (0..width * height).flat_map(|i| [(i % 251) as u8, (i / 7 % 256) as u8, (i % 13 * 19) as u8, 0]).collect();
        let started = std::time::Instant::now();
        let picture = unsafe { Picture::copy(rows.as_ptr(), width as usize * 4, width, height) }.unwrap();
        let (png, ..) = picture.png(100).unwrap();
        eprintln!("1920x1080 copied and encoded in {:?}: {} bytes", started.elapsed(), png.len());
        let (w, h, pixels) = decode(&png);
        assert_eq!((w, h), (width, height));
        for i in [0usize, 1, 12_345, (width * height - 1) as usize] {
            assert_eq!(&pixels[i * 3..i * 3 + 3], &rows[i * 4..i * 4 + 3], "pixel {i}");
        }
    }
}
