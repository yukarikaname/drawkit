// The zero-copy WinUI path. A SwapChainPanel owns the swapchain; the Rust surface draws the paper
// and the resident layers directly, so a stroke never reads pixels back into a WriteableBitmap.

using System;
using System.Runtime.InteropServices;

namespace DrawKit;

internal sealed partial class PaintEngine
{
    private IntPtr _surface;
    private IntPtr _panelNative;
    private bool _surfaceReady;
    private int _surfaceLayerCount;
    private int _surfaceWidth;
    private int _surfaceHeight;
    private float[] _linear = Array.Empty<float>();
    private float[] _patch = Array.Empty<float>();

    public bool HasSurface => _surface != IntPtr.Zero;
    public bool IsHdr => Native.dk_dynamic_range(_handle) != 0;
    /// True when the window system really took a float swapchain.
    public bool HdrSurface => _surface != IntPtr.Zero && GpuSurfaceNative.dk_canvas_surface_hdr(_surface) != 0;
    public float Exposure { get; private set; }

    /// Bind the WinUI SwapChainPanel's native interface. The panel keeps the swapchain alive;
    /// the returned surface draws the paper and the resident layers with no readback.
    public bool AttachSwapChainPanel(object panel, uint width, uint height)
    {
        if (_surface != IntPtr.Zero) return true;
        if (width == 0 || height == 0) return false;
        var unknown = IntPtr.Zero;
        var native = IntPtr.Zero;
        try
        {
            unknown = Marshal.GetIUnknownForObject(panel);
            var iid = new Guid("63aad0b8-7c24-40ff-85a8-640d944cc325");
            if (Marshal.QueryInterface(unknown, ref iid, out native) != 0) return false;
            _surface = CreateSurface(native, width, height, IsHdr);
            if (_surface == IntPtr.Zero) return false;
            _panelNative = native;
            native = IntPtr.Zero;
            _surfaceWidth = (int)width;
            _surfaceHeight = (int)height;
            GpuSurfaceNative.dk_canvas_surface_set_exposure(_surface, Exposure);
            FullComposite();
            return _surfaceReady;
        }
        catch (DllNotFoundException) { _surface = IntPtr.Zero; }
        catch (EntryPointNotFoundException) { _surface = IntPtr.Zero; }
        catch (BadImageFormatException) { _surface = IntPtr.Zero; }
        finally
        {
            if (unknown != IntPtr.Zero) Marshal.Release(unknown);
            if (native != IntPtr.Zero) Marshal.Release(native);
        }
        return false;
    }

    public void ResizeSurface(uint width, uint height)
    {
        if (_surface == IntPtr.Zero || width == 0 || height == 0) return;
        _surfaceWidth = (int)width;
        _surfaceHeight = (int)height;
        GpuSurfaceNative.dk_canvas_surface_resize(_surface, width, height);
        PresentSurface();
    }

    private bool SyncSurface()
    {
        var count = (int)Native.dk_layer_count(_handle);
        if (count <= 0) return false;
        for (var index = 0; index < count; index++)
        {
            var kind = Native.dk_layer_kind(_handle, (uint)index);
            if (Native.dk_layer_clipping(_handle, (uint)index) != 0 &&
                Native.dk_layer_clip_base(_handle, (uint)index) < 0)
            {
                _surfaceReady = false;
                return false;
            }
            if (kind == 1 &&
                (Native.dk_layer_opacity(_handle, (uint)index) < 0.999f ||
                 Native.dk_layer_blend(_handle, (uint)index) != 0 ||
                 Native.dk_layer_clipping(_handle, (uint)index) != 0))
            {
                _surfaceReady = false;
                return false;
            }
            if (Native.dk_layer_parent(_handle, (uint)index) >= 0 &&
                Native.dk_layer_blend(_handle, (uint)index) != 0)
            {
                _surfaceReady = false;
                return false;
            }
        }
        GpuSurfaceNative.dk_canvas_surface_reset_layers(
            _surface, (uint)count, (uint)Width, (uint)Height);
        GpuSurfaceNative.dk_canvas_surface_set_layer_budget(_surface, 1536UL << 20);
        var full = new byte[Width * Height * 4];
        for (var index = 0; index < count; index++)
        {
            if (Native.dk_layer_kind(_handle, (uint)index) == 0)
            {
                Array.Clear(full, 0, full.Length);
                if (Native.dk_layer_rect(_handle, (uint)index, 0, 0, (uint)Width, (uint)Height,
                        full) == 0) return false;
                if (GpuSurfaceNative.dk_canvas_surface_upload_layer(_surface, (uint)index, 0, 0,
                        (uint)Width, (uint)Height, full) == 0) return false;
            }
        }
        _surfaceLayerCount = count;
        SetSurfaceSpecs();
        _surfaceReady = true;
        return true;
    }

    private void SetSurfaceSpecs()
    {
        var count = _surfaceLayerCount;
        var colour = new float[4];
        for (var index = 0; index < count; index++)
        {
            var kind = Native.dk_layer_kind(_handle, (uint)index);
            var fill = 0;
            Array.Clear(colour, 0, colour.Length);
            if (kind == 2 && Native.dk_layer_fill(_handle, (uint)index, colour) != 0) fill = 1;
            GpuSurfaceNative.dk_canvas_surface_set_layer(_surface, (uint)index,
                Native.dk_layer_effective_opacity(_handle, (uint)index),
                Native.dk_layer_blend(_handle, (uint)index),
                Native.dk_layer_effective_visible(_handle, (uint)index) != 0 ? 1 : 0,
                fill, colour[0], colour[1], colour[2], colour[3]);
            var clip = Native.dk_layer_clipping(_handle, (uint)index) != 0
                ? Native.dk_layer_clip_base(_handle, (uint)index)
                : -1;
            GpuSurfaceNative.dk_canvas_surface_set_layer_clip(_surface, (uint)index, clip);
        }
    }

    /// Turn extended range on for this document. The display path is chosen by the wgpu surface:
    /// extended Display P3 on Apple, scRGB on Windows, BT.2100 PQ on a Wayland compositor that
    /// advertises it, and the eight-bit path only when the window system has no float swapchain.
    public void SetHdr(bool enabled)
    {
        Native.dk_set_dynamic_range(_handle, enabled ? 1 : 0);
        _linear = enabled ? new float[Width * Height * 4] : Array.Empty<float>();
        _surfaceReady = false;
        _gpuReady = false;
        RecreateSurface(enabled);
        FullComposite();
    }

    private IntPtr CreateSurface(IntPtr panel, uint width, uint height, bool hdr)
    {
        return GpuSurfaceNative.dk_canvas_surface_new_native(
            IntPtr.Zero, (ulong)panel.ToInt64(), 0, width, height, hdr ? 1 : 0, 0);
    }

    /// A SDR surface uses the resident eight-bit layers; a HDR document needs the float drawable.
    /// The panel outlives the surface, so a toggle can free and recreate it in place.
    private void RecreateSurface(bool hdr)
    {
        if (_surface != IntPtr.Zero)
        {
            GpuSurfaceNative.dk_canvas_surface_free(_surface);
            _surface = IntPtr.Zero;
        }
        if (_panelNative == IntPtr.Zero || _surfaceWidth <= 0 || _surfaceHeight <= 0) return;
        _surface = CreateSurface(_panelNative, (uint)_surfaceWidth, (uint)_surfaceHeight, hdr);
        if (_surface == IntPtr.Zero) return;
        GpuSurfaceNative.dk_canvas_surface_set_exposure(_surface, Exposure);
    }

    /// Display exposure in stops. It changes the presentation, not the stored pixels.
    public void SetExposure(float stops)
    {
        Exposure = stops;
        if (_surface != IntPtr.Zero)
            GpuSurfaceNative.dk_canvas_surface_set_exposure(_surface, stops);
        FullComposite();
    }

    private void EnsureLinear()
    {
        var count = Width * Height * 4;
        if (_linear.Length != count) _linear = new float[count];
    }

    private void FullHdr()
    {
        EnsureLinear();
        if (Native.dk_composite_linear(_handle, _linear) == 0) return;
        if (PresentHdr(0, 0, 0, 0)) return;
        // No float swapchain: present the encoded eight-bit composite. Nothing is tone mapped;
        // values above paper white clip exactly as they would in any SDR viewer.
        var pixels = new byte[Width * Height * 4];
        Native.dk_composite(_handle, pixels);
        Copy(pixels, 0, 0, Width, Height);
        Bitmap.Invalidate();
    }

    private void DirtyHdr(int x, int y, int width, int height)
    {
        EnsureLinear();
        var count = width * height * 4;
        if (_patch.Length < count) _patch = new float[count];
        if (Native.dk_composite_linear_rect(_handle, (uint)x, (uint)y,
                (uint)width, (uint)height, _patch) != 0)
        {
            for (var row = 0; row < height; row++)
            {
                Array.Copy(_patch, row * width * 4,
                    _linear, ((y + row) * Width + x) * 4, width * 4);
            }
            if (PresentHdr(x, y, width, height)) return;
        }
        var pixels = new byte[count];
        Native.dk_composite_rect(_handle, (uint)x, (uint)y, (uint)width, (uint)height, pixels);
        Copy(pixels, x, y, width, height);
        Bitmap.Invalidate();
    }

    private bool PresentHdr(int x, int y, int width, int height)
    {
        if (_surface == IntPtr.Zero || !HdrSurface || _linear.Length == 0) return false;
        ComputePaper(out var paperX, out var paperY, out var paperWidth, out var paperHeight);
        return GpuSurfaceNative.dk_canvas_surface_present_hdr(
            _surface, _linear, (uint)Width, (uint)Height,
            paperX, paperY, paperWidth, paperHeight,
            (uint)x, (uint)y, (uint)width, (uint)height) != 0;
    }

    private void ComputePaper(out float paperX, out float paperY,
                              out float paperWidth, out float paperHeight)
    {
        var scale = Math.Min(_surfaceWidth / (float)Width, _surfaceHeight / (float)Height);
        paperWidth = Width * scale;
        paperHeight = Height * scale;
        paperX = (_surfaceWidth - paperWidth) * 0.5f;
        paperY = (_surfaceHeight - paperHeight) * 0.5f;
    }

    private bool PresentSurface()
    {
        if (_surface == IntPtr.Zero || _surfaceWidth <= 0 || _surfaceHeight <= 0) return false;
        ComputePaper(out var paperX, out var paperY, out var paperWidth, out var paperHeight);
        return GpuSurfaceNative.dk_canvas_surface_present_layers(
            _surface, paperX, paperY, paperWidth, paperHeight) != 0;
    }
}
