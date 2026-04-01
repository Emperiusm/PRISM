/// Desktop Duplication API capture backend for Windows.
///
/// Uses DXGI Desktop Duplication to capture the primary display into CPU-accessible
/// BGRA pixel buffers at screen resolution.
#[cfg(windows)]
pub mod dda_capture {
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
        DXGI_ERROR_WAIT_TIMEOUT,
    };
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_STAGING,
    };
    use windows::core::{Interface, Result};

    pub struct DdaDesktopCapture {
        /// Held alive to keep the D3D11 device alive for the lifetime of the duplication.
        #[allow(dead_code)]
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        duplication: IDXGIOutputDuplication,
        width: u32,
        height: u32,
        staging_texture: ID3D11Texture2D,
    }

    impl DdaDesktopCapture {
        /// Initialise DDA capture on the primary adapter / primary output (output 0).
        pub fn new() -> Result<Self> {
            Self::new_with_output(0)
        }

        /// Initialise DDA capture on the primary adapter, selecting `output_index`.
        ///
        /// `output_index` is the zero-based index of the DXGI output (monitor) on
        /// adapter 0.  If the index exceeds the number of connected outputs, the
        /// underlying `EnumOutputs` call will return an error.
        pub fn new_with_output(output_index: u32) -> Result<Self> {
            // 1. Create DXGI factory
            let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };

            // 2. Get first adapter
            let adapter = unsafe { factory.EnumAdapters1(0)? };

            // 3. Create D3D11 device + immediate context
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            unsafe {
                D3D11CreateDevice(
                    &adapter,                            // pAdapter: Param<IDXGIAdapter>
                    D3D_DRIVER_TYPE_UNKNOWN,             // DriverType
                    None,                                // Software module (unused with explicit adapter)
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,   // Flags
                    None,                                // pFeatureLevels (use defaults)
                    D3D11_SDK_VERSION,                   // SDKVersion
                    Some(&mut device),                   // ppDevice
                    None,                                // pFeatureLevel (don't care)
                    Some(&mut context),                  // ppImmediateContext
                )?;
            }
            let device = device.expect("D3D11CreateDevice succeeded but returned no device");
            let context = context.expect("D3D11CreateDevice succeeded but returned no context");

            // 4. Get the selected output and cast to IDXGIOutput1
            let output = unsafe { adapter.EnumOutputs(output_index)? };
            let output1: IDXGIOutput1 = output.cast()?;

            // 5. Get output dimensions before duplicating
            let desc = unsafe { output.GetDesc()? };
            let width =
                (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
            let height =
                (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;

            // 6. Duplicate the output (requires the D3D11 device)
            let duplication = unsafe { output1.DuplicateOutput(&device)? };

            // 7. Create a CPU-readable staging texture for readback
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: D3D11_BIND_FLAG(0).0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging_texture: Option<ID3D11Texture2D> = None;
            unsafe {
                device.CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))?;
            }
            let staging_texture =
                staging_texture.expect("CreateTexture2D succeeded but returned no texture");

            Ok(Self {
                device,
                context,
                duplication,
                width,
                height,
                staging_texture,
            })
        }

        pub fn width(&self) -> u32 {
            self.width
        }

        pub fn height(&self) -> u32 {
            self.height
        }

        /// Capture one desktop frame.
        ///
        /// Returns `Ok(Some(bgra))` when a new frame was acquired,
        /// `Ok(None)` on timeout (no new frame within 100 ms), or
        /// `Err(_)` on a hard error.
        pub fn capture_frame(&self) -> Result<Option<Vec<u8>>> {
            use windows::Win32::Graphics::Dxgi::DXGI_OUTDUPL_FRAME_INFO;

            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;

            let result = unsafe {
                self.duplication
                    .AcquireNextFrame(100, &mut frame_info, &mut resource)
            };

            match result {
                Ok(()) => {}
                Err(ref e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(None),
                Err(e) => return Err(e),
            }

            let resource = resource.expect("AcquireNextFrame succeeded but returned no resource");
            let texture: ID3D11Texture2D = resource.cast()?;

            // Copy the GPU texture to the CPU-accessible staging texture
            unsafe {
                self.context.CopyResource(&self.staging_texture, &texture);
            }

            // Map the staging texture to read pixel data
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                self.context
                    .Map(&self.staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            }

            // Copy rows, accounting for GPU row pitch vs. tightly-packed width
            let row_bytes = (self.width * 4) as usize;
            let mut pixels = vec![0u8; row_bytes * self.height as usize];
            let src_base = mapped.pData as *const u8;
            for y in 0..self.height as usize {
                let src_row =
                    unsafe { src_base.add(y * mapped.RowPitch as usize) };
                let dst_start = y * row_bytes;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src_row,
                        pixels[dst_start..].as_mut_ptr(),
                        row_bytes,
                    );
                }
            }

            unsafe { self.context.Unmap(&self.staging_texture, 0) };
            unsafe { self.duplication.ReleaseFrame()? };

            Ok(Some(pixels))
        }
    }

    impl Drop for DdaDesktopCapture {
        fn drop(&mut self) {
            // COM interface references are automatically released by the windows crate Drop impls.
        }
    }
}
