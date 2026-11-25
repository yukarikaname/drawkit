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

    public bool HasSurface => _surface != IntPtr.Zero;

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
            _surface = GpuSurfaceNative.dk_canvas_surface_new_native(
                IntPtr.Zero, (ulong)native.ToInt64(), 0, width, height, 0, 0);
            if (_surface == IntPtr.Zero) return false;
            _panelNative = native;
            native = IntPtr.Zero;
            _surfaceWidth = (int)width;
            _surfaceHeight = (int)height;
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

    private bool PresentSurface()
    {
        if (_surface == IntPtr.Zero || _surfaceWidth <= 0 || _surfaceHeight <= 0) return false;
        var scale = Math.Min(_surfaceWidth / (float)Width, _surfaceHeight / (float)Height);
        var paperWidth = Width * scale;
        var paperHeight = Height * scale;
        var paperX = (_surfaceWidth - paperWidth) * 0.5f;
        var paperY = (_surfaceHeight - paperHeight) * 0.5f;
        return GpuSurfaceNative.dk_canvas_surface_present_layers(
            _surface, paperX, paperY, paperWidth, paperHeight) != 0;
    }
}
