// The drawing engine on the Windows side: one native handle, one BGRA bitmap, and no pixel copy
// beyond the rectangle the core dirtied.

using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml.Media.Imaging;

namespace DrawKit;

internal sealed partial class PaintEngine : IDisposable
{
    private IntPtr _handle;
    private IntPtr _gpu;
    private bool _gpuReady;
    private int _gpuLayerCount;
    private byte[] _gpuPixels = Array.Empty<byte>();
    private ulong _time;

    public int Width { get; private set; }
    public int Height { get; private set; }
    public WriteableBitmap Bitmap { get; private set; }

    public PaintEngine(int width, int height)
    {
        Width = width;
        Height = height;
        _handle = Native.dk_editor_new((uint)width, (uint)height);
        Bitmap = new WriteableBitmap(width, height);
        ResetGpu(width, height);
        FullComposite();
    }

    public bool Open(string path)
    {
        var opened = Native.dk_open_file(path);
        if (opened == IntPtr.Zero) return false;
        Native.dk_editor_free(_handle);
        _handle = opened;
        Width = (int)Native.dk_width(_handle);
        Height = (int)Native.dk_height(_handle);
        Bitmap = new WriteableBitmap(Width, Height);
        _linear = Array.Empty<float>();
        _patch = Array.Empty<float>();
        ResetGpu(Width, Height);
        RecreateSurface(IsHdr);
        FullComposite();
        return true;
    }

    public bool Save(string path) => Native.dk_save_tape(_handle, path) != 0;
    public bool SaveExr(string path) => Native.dk_export_exr(_handle, path) != 0;

    public void Begin()
    {
        _time = Math.Max(_time + 1, (ulong)Environment.TickCount64);
        Native.dk_stroke_begin(_handle, _time);
    }

    public void Segment(double x0, double y0, double x1, double y1,
                        float radius, float pressure,
                        float tiltX, float tiltY, float azimuth, float roll,
                        float[] color, bool erase)
    {
        _time = Math.Max(_time + 1, (ulong)Environment.TickCount64);
        if (erase)
        {
            Native.dk_erase_ex(_handle, (float)x0, (float)y0, (float)x1, (float)y1,
                radius, pressure, tiltX, tiltY, azimuth, roll, _time);
        }
        else
        {
            Native.dk_stroke_ex(_handle, (float)x0, (float)y0, (float)x1, (float)y1,
                radius, pressure, tiltX, tiltY, azimuth, roll,
                color[0], color[1], color[2], color[3], _time);
        }
        ApplyDirty();
    }

    public void End() => Native.dk_stroke_end(_handle);

    public void Fill(int x, int y, float tolerance, int close, int expand, int feather,
                     float[] color)
    {
        Native.dk_fill_ex(_handle, (uint)x, (uint)y, tolerance,
            (uint)close, (uint)expand, (uint)feather,
            color[0], color[1], color[2], color[3], (ulong)Environment.TickCount64);
        ApplyDirty();
    }

    public bool BeginFill(int x, int y, float tolerance, int close, int expand, int feather,
                          float[] color)
    {
        var started = Native.dk_fill_begin(_handle, (uint)x, (uint)y, tolerance,
            (uint)close, (uint)expand, (uint)feather,
            color[0], color[1], color[2], color[3], (ulong)Environment.TickCount64) != 0;
        ApplyDirty();
        return started;
    }

    public bool FillStep(float progress)
    {
        if (Native.dk_fill_step(_handle, progress) == 0) return false;
        ApplyDirty();
        return true;
    }

    public bool FinishFill()
    {
        if (Native.dk_fill_finish(_handle) == 0) return false;
        ApplyDirty();
        return true;
    }

    public void CancelFill()
    {
        if (Native.dk_fill_cancel(_handle) != 0) ApplyDirty();
    }

    public bool ExtractLineArt()
    {
        if (Native.dk_extract_lineart(_handle, 0.55f, 0.12f) == 0) return false;
        FullComposite();
        return true;
    }

    public bool Undo()
    {
        if (Native.dk_undo(_handle) == 0) return false;
        FullComposite();
        return true;
    }

    public bool Redo()
    {
        if (Native.dk_redo(_handle) == 0) return false;
        FullComposite();
        return true;
    }

    public bool CanUndo => Native.dk_can_undo(_handle) != 0;
    public bool CanRedo => Native.dk_can_redo(_handle) != 0;
    public bool IsBlank => Native.dk_is_clear(_handle) != 0;

    public void SetMirror(int mode) => Native.dk_set_mirror(_handle, mode);
    public void SetMirrorAngle(float degrees, bool both) =>
        Native.dk_set_mirror_angle(_handle, degrees, both ? 1 : 0);
    public void SetStabilization(float strength) => Native.dk_set_stabilization(_handle, strength);
    public void SetAutoShape(bool enabled) => Native.dk_set_auto_shape(_handle, enabled ? 1 : 0);

    public float[]? Pick(int x, int y)
    {
        var color = new float[4];
        return Native.dk_pick(_handle, (uint)x, (uint)y, color) != 0 ? color : null;
    }

    public bool AutoShape()
    {
        if (Native.dk_auto_shape(_handle) == 0) return false;
        ApplyDirty();
        return true;
    }

    public List<LayerNode> Layers()
    {
        var result = new List<LayerNode>();
        var count = Native.dk_layer_count(_handle);
        for (uint index = 0; index < count; index++)
        {
            var name = new byte[256];
            var written = Native.dk_layer_name(_handle, index, name, (uint)name.Length);
            var parent = Native.dk_layer_parent(_handle, index);
            result.Add(new LayerNode(
                index,
                written > 0 ? System.Text.Encoding.UTF8.GetString(name, 0, written) : $"Layer {index + 1}",
                Native.dk_layer_visible(_handle, index) != 0,
                Native.dk_layer_opacity(_handle, index),
                Native.dk_layer_kind(_handle, index),
                parent < 0 ? null : (int)parent,
                Native.dk_layer_expanded(_handle, index) != 0,
                Native.dk_layer_tag(_handle, index)));
        }
        return result;
    }

    public int ActiveLayer => (int)Native.dk_layer_active(_handle);
    public void SelectLayer(int index) => Native.dk_layer_select(_handle, (uint)index);
    public void SetVisible(int index, bool visible)
    {
        Native.dk_layer_set_visible(_handle, (uint)index, visible ? 1 : 0);
        RefreshSpecs();
    }

    public void SetOpacity(int index, float opacity)
    {
        Native.dk_layer_set_opacity(_handle, (uint)index, opacity);
        RefreshSpecs();
    }
    public void SetTag(int index, int tag) =>
        Native.dk_layer_set_tag(_handle, (uint)index, tag);
    public void SetExpanded(int index, bool expanded) =>
        Native.dk_layer_set_expanded(_handle, (uint)index, expanded ? 1 : 0);
    public void AddLayer() { Native.dk_layer_add(_handle); FullComposite(); }
    public void AddGroup() { Native.dk_layer_add_group(_handle); FullComposite(); }
    public void AddFill(float[] color)
    {
        Native.dk_layer_add_fill(_handle, color[0], color[1], color[2], color[3]);
        FullComposite();
    }
    public void RemoveLayer() { Native.dk_layer_remove(_handle); FullComposite(); }
    public void MoveLayer(int index, int steps) => Native.dk_layer_move(_handle, (uint)index, steps);
    public void RenameLayer(int index, string name) =>
        Native.dk_layer_set_name(_handle, (uint)index, name);

    private void FullComposite()
    {
        if (IsHdr)
        {
            FullHdr();
            return;
        }
        if (_surface != IntPtr.Zero && SyncSurface() && PresentSurface())
        {
            return;
        }
        if (_gpu != IntPtr.Zero && SyncGpu())
        {
            if (CompositeGpu())
            {
                Copy(_gpuPixels, 0, 0, Width, Height);
                Bitmap.Invalidate();
                return;
            }
            _gpuReady = false;
        }
        var pixels = new byte[Width * Height * 4];
        Native.dk_composite(_handle, pixels);
        Copy(pixels, 0, 0, Width, Height);
        Bitmap.Invalidate();
    }

    private void ApplyDirty()
    {
        var dirty = new uint[4];
        if (Native.dk_take_dirty(_handle, dirty) == 0) return;
        var x = (int)dirty[0];
        var y = (int)dirty[1];
        var width = (int)dirty[2];
        var height = (int)dirty[3];
        if (width <= 0 || height <= 0) return;
        if (IsHdr)
        {
            DirtyHdr(x, y, width, height);
            return;
        }
        if (_surface != IntPtr.Zero && _surfaceReady)
        {
            var active = (uint)ActiveLayer;
            var patch = new byte[width * height * 4];
            if (Native.dk_layer_rect(_handle, active, (uint)x, (uint)y,
                    (uint)width, (uint)height, patch) != 0
                && GpuSurfaceNative.dk_canvas_surface_upload_layer(
                    _surface, active, (uint)x, (uint)y, (uint)width, (uint)height, patch) != 0
                && PresentSurface())
            {
                return;
            }
            _surfaceReady = false;
        }
        if (_gpu != IntPtr.Zero && _gpuReady)
        {
            var active = (uint)ActiveLayer;
            var patch = new byte[width * height * 4];
            if (Native.dk_layer_rect(_handle, active, (uint)x, (uint)y,
                    (uint)width, (uint)height, patch) != 0
                && GpuNative.dk_gpu_compositor_upload_layer(
                    _gpu, active, (uint)x, (uint)y, (uint)width, (uint)height, patch) != 0
                && CompositeGpu())
            {
                Copy(_gpuPixels, x, y, width, height);
                Bitmap.Invalidate();
                return;
            }
            _gpuReady = false;
        }
        var pixels = new byte[width * height * 4];
        Native.dk_composite_rect(_handle, (uint)x, (uint)y, (uint)width, (uint)height, pixels);
        Copy(pixels, x, y, width, height);
        Bitmap.Invalidate();
    }

    private void ResetGpu(int width, int height)
    {
        if (_gpu != IntPtr.Zero)
        {
            GpuNative.dk_gpu_compositor_free(_gpu);
            _gpu = IntPtr.Zero;
        }
        _gpuReady = false;
        _gpuPixels = new byte[width * height * 4];
        try
        {
            _gpu = GpuNative.dk_gpu_compositor_new((uint)width, (uint)height);
            if (_gpu != IntPtr.Zero)
            {
                GpuNative.dk_gpu_compositor_set_budget(_gpu, 1536UL << 20);
            }
        }
        catch (DllNotFoundException) { _gpu = IntPtr.Zero; }
        catch (EntryPointNotFoundException) { _gpu = IntPtr.Zero; }
        catch (BadImageFormatException) { _gpu = IntPtr.Zero; }
    }

    /// Upload the whole stack before a full composite. Groups and clipping still go through the
    /// core's CPU path: the resident compositor does not reproduce their isolation.
    private bool SyncGpu()
    {
        var count = (int)Native.dk_layer_count(_handle);
        if (count <= 0) return false;
        for (var index = 0; index < count; index++)
        {
            var kind = Native.dk_layer_kind(_handle, (uint)index);
            if (Native.dk_layer_clipping(_handle, (uint)index) != 0 &&
                Native.dk_layer_clip_base(_handle, (uint)index) < 0)
            {
                _gpuReady = false;
                return false;
            }
            // A group is flattened only when it is a plain normal group at full opacity: its
            // children then see the same result on the GPU as the isolated CPU group.
            if (kind == 1 &&
                (Native.dk_layer_opacity(_handle, (uint)index) < 0.999f ||
                 Native.dk_layer_blend(_handle, (uint)index) != 0 ||
                 Native.dk_layer_clipping(_handle, (uint)index) != 0))
            {
                _gpuReady = false;
                return false;
            }
            if (Native.dk_layer_parent(_handle, (uint)index) >= 0 &&
                Native.dk_layer_blend(_handle, (uint)index) != 0)
            {
                _gpuReady = false;
                return false;
            }
        }
        GpuNative.dk_gpu_compositor_reset(_gpu, (uint)count, (uint)Width, (uint)Height);
        var full = new byte[Width * Height * 4];
        for (var index = 0; index < count; index++)
        {
            if (Native.dk_layer_kind(_handle, (uint)index) == 0)
            {
                Array.Clear(full, 0, full.Length);
                if (Native.dk_layer_rect(_handle, (uint)index, 0, 0, (uint)Width, (uint)Height,
                        full) == 0) return false;
                if (GpuNative.dk_gpu_compositor_upload_layer(_gpu, (uint)index, 0, 0,
                        (uint)Width, (uint)Height, full) == 0) return false;
            }
        }
        _gpuLayerCount = count;
        SetGpuSpecs();
        _gpuReady = true;
        return true;
    }

    /// Opacity and visibility only change the spec, not the pixels. Reuse the resident layers and
    /// fall back to a full sync if the stack changed behind us.
    private void RefreshSpecs()
    {
        if (_surface != IntPtr.Zero && _surfaceReady &&
            Native.dk_layer_count(_handle) == (uint)_surfaceLayerCount)
        {
            SetSurfaceSpecs();
            if (PresentSurface()) return;
            _surfaceReady = false;
        }
        if (_gpu != IntPtr.Zero && _gpuReady &&
            Native.dk_layer_count(_handle) == (uint)_gpuLayerCount)
        {
            SetGpuSpecs();
            if (CompositeGpu())
            {
                Copy(_gpuPixels, 0, 0, Width, Height);
                Bitmap.Invalidate();
                return;
            }
        }
        FullComposite();
    }

    private void SetGpuSpecs()
    {
        var count = _gpuLayerCount;
        var colour = new float[4];
        for (var index = 0; index < count; index++)
        {
            var kind = Native.dk_layer_kind(_handle, (uint)index);
            var fill = 0;
            Array.Clear(colour, 0, colour.Length);
            if (kind == 2 && Native.dk_layer_fill(_handle, (uint)index, colour) != 0) fill = 1;
            GpuNative.dk_gpu_compositor_set_layer(_gpu, (uint)index,
                Native.dk_layer_effective_opacity(_handle, (uint)index),
                Native.dk_layer_blend(_handle, (uint)index),
                Native.dk_layer_effective_visible(_handle, (uint)index) != 0 ? 1 : 0,
                fill, colour[0], colour[1], colour[2], colour[3]);
            var clip = Native.dk_layer_clipping(_handle, (uint)index) != 0
                ? Native.dk_layer_clip_base(_handle, (uint)index)
                : -1;
            GpuNative.dk_gpu_compositor_set_layer_clip(_gpu, (uint)index, clip);
        }
    }

    private bool CompositeGpu()
    {
        return GpuNative.dk_gpu_compositor_composite(_gpu, _gpuPixels,
            (uint)Width, (uint)Height) != 0;
    }

    /// RGBA premultiplied from the core to BGRA over white for the bitmap.
    private void Copy(byte[] rgba, int x, int y, int width, int height)
    {
        using var stream = Bitmap.PixelBuffer.AsStream();
        for (var row = 0; row < height; row++)
        {
            stream.Seek(((long)(y + row) * Width + x) * 4, SeekOrigin.Begin);
            for (var column = 0; column < width; column++)
            {
                var index = (row * width + column) * 4;
                var alpha = rgba[index + 3];
                var paper = 255 - alpha;
                stream.WriteByte((byte)Math.Min(255, rgba[index + 2] + paper));
                stream.WriteByte((byte)Math.Min(255, rgba[index + 1] + paper));
                stream.WriteByte((byte)Math.Min(255, rgba[index] + paper));
                stream.WriteByte(255);
            }
        }
    }

    public void Dispose()
    {
        if (_surface != IntPtr.Zero)
        {
            GpuSurfaceNative.dk_canvas_surface_free(_surface);
            _surface = IntPtr.Zero;
        }
        if (_panelNative != IntPtr.Zero)
        {
            Marshal.Release(_panelNative);
            _panelNative = IntPtr.Zero;
        }
        if (_gpu != IntPtr.Zero)
        {
            GpuNative.dk_gpu_compositor_free(_gpu);
            _gpu = IntPtr.Zero;
        }
        if (_handle != IntPtr.Zero)
        {
            Native.dk_editor_free(_handle);
            _handle = IntPtr.Zero;
        }
    }
}

public sealed record LayerNode(
    uint Index, string Name, bool Visible, double Opacity,
    int Kind, int? Parent, bool Expanded, int Tag);
