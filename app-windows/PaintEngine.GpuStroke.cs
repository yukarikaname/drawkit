// The GPU stroke path. The coverage preview lives on the surface, and pen-up composites it into
// the resident tiles. Undo swaps tile textures; the CPU bitmap is rebuilt only when something reads
// it, so a stroke never copies pixels back.

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace DrawKit;

internal static class StrokeNative
{
    private const string Library = "drawkit_core";

    [DllImport(Library)] internal static extern void dk_set_defer_raster(IntPtr handle, int enabled);
    [DllImport(Library)] internal static extern void dk_flush_deferred(IntPtr handle);
    [DllImport(Library)] internal static extern float dk_brush_param(IntPtr handle, uint id);
    [DllImport(Library)] internal static extern float dk_brush_kind_param(int kind, uint id);
    [DllImport(Library)] internal static extern uint dk_layer_active(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_stroke_tile_begin(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_stroke_gpu_end(IntPtr handle, uint[]? rects,
        uint count);
    [DllImport(Library)] internal static extern int dk_stroke_apply_tiles(IntPtr handle, uint layer,
        uint x, uint y, uint width, uint height, byte[] pixels);
    [DllImport(Library)] internal static extern int dk_stroke_seed(IntPtr handle, uint[] halves);
    [DllImport(Library)] internal static extern int dk_brush_texture_size(IntPtr handle,
        uint[] size);
    [DllImport(Library)] internal static extern int dk_brush_texture_preview(IntPtr handle,
        uint width, uint height, byte[] outPixels);
    [DllImport(Library)] internal static extern int dk_mirror_segments(IntPtr handle,
        float fromX, float fromY, float toX, float toY, float[] buffer);
    [DllImport(Library)] internal static extern int dk_undo_is_gpu(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_redo_is_gpu(IntPtr handle);
}

internal static class StrokeSurfaceNative
{
    private const string Library = "drawkit_gpu";

    [DllImport(Library)] internal static extern void dk_canvas_surface_stroke_begin(IntPtr surface,
        uint width, uint height, float red, float green, float blue, float alpha,
        float flow, float pressureAlpha, float paper, float tip,
        float dual, float dualScale, float dualHardness, float dualRoundness,
        float dualAngle, float dualPaper, float dualTip, float wet,
        float jitter, float grain, float dualJitter, float dualGrain,
        uint seedLow, uint seedHigh);
    [DllImport(Library)] internal static extern void dk_canvas_surface_stroke_segment(IntPtr surface,
        float fromX, float fromY, float toX, float toY, float radius, float hardness,
        float spacing, float roundness, float angle, float pressure);
    [DllImport(Library)] internal static extern int dk_canvas_surface_stroke_commit(IntPtr surface,
        uint layer, float red, float green, float blue, float flow);
    [DllImport(Library)] internal static extern int dk_canvas_surface_committed_rect(IntPtr surface,
        uint index, uint[] rect);
    [DllImport(Library)] internal static extern int dk_canvas_surface_set_brush_texture(
        IntPtr surface, uint width, uint height, byte[] data, uint length);
    [DllImport(Library)] internal static extern void dk_canvas_surface_clear_brush_texture(
        IntPtr surface);
    [DllImport(Library)] internal static extern void dk_canvas_surface_stroke_end(IntPtr surface);
    [DllImport(Library)] internal static extern int dk_canvas_surface_undo_tiles(IntPtr surface);
    [DllImport(Library)] internal static extern int dk_canvas_surface_redo_tiles(IntPtr surface);
    [DllImport(Library)] internal static extern int dk_canvas_surface_materialize(IntPtr surface);
    [DllImport(Library)] internal static extern int dk_canvas_surface_committed_info(IntPtr surface,
        uint index, uint[] rect);
    [DllImport(Library)] internal static extern int dk_canvas_surface_committed_tile(IntPtr surface,
        uint index, uint[] rect, byte[] pixels, uint length);
    [DllImport(Library)] internal static extern int dk_canvas_surface_committed_layer(IntPtr surface,
        uint index);
    [DllImport(Library)] internal static extern void dk_canvas_surface_clear_committed(IntPtr surface);
}

internal sealed partial class PaintEngine
{
    private bool _gpuWanted;
    private bool _gpuStroke;
    private bool _gpuStale;
    private float[] _gpuInk = Array.Empty<float>();

    private bool GpuStrokeAvailable => _surface != IntPtr.Zero && _surfaceReady && !IsHdr;

    /// Decide at pen-down whether this stroke can stay on the GPU. An eraser or an auto shape needs
    /// the CPU result, so those keep the ordinary path.
    public void GpuStrokePrepare(bool erase, bool autoShape)
    {
        _gpuWanted = !erase && !autoShape && GpuStrokeAvailable;
        if (_gpuWanted)
        {
            StrokeNative.dk_set_defer_raster(_handle, 1);
            UploadBrushMap();
        }
    }

    private void UploadBrushMap()
    {
        var size = new uint[2];
        if (StrokeNative.dk_brush_texture_size(_handle, size) != 0 && size[0] > 0 && size[1] > 0)
        {
            var data = new byte[size[0] * size[1]];
            if (StrokeNative.dk_brush_texture_preview(_handle, size[0], size[1], data) != 0)
            {
                StrokeSurfaceNative.dk_canvas_surface_set_brush_texture(_surface,
                    size[0], size[1], data, (uint)data.Length);
                return;
            }
        }
        StrokeSurfaceNative.dk_canvas_surface_clear_brush_texture(_surface);
    }

    /// Accumulate one segment's coverage on the GPU. The core only records the event.
    public void GpuStrokeDraw(double x0, double y0, double x1, double y1,
                              float radius, float pressure,
                              float tiltX, float tiltY, float azimuth, float[] color)
    {
        if (!_gpuWanted) return;
        if (!_gpuStroke)
        {
            _gpuInk = color;
            var kind = (int)StrokeNative.dk_brush_param(_handle, 14);
            var seedHalves = new uint[2];
            _ = StrokeNative.dk_stroke_seed(_handle, seedHalves);
            StrokeSurfaceNative.dk_canvas_surface_stroke_begin(_surface,
                (uint)Width, (uint)Height, color[0], color[1], color[2], color[3],
                StrokeNative.dk_brush_param(_handle, 1), StrokeNative.dk_brush_param(_handle, 3),
                StrokeNative.dk_brush_param(_handle, 9), StrokeNative.dk_brush_param(_handle, 10),
                StrokeNative.dk_brush_param(_handle, 12), StrokeNative.dk_brush_param(_handle, 13),
                StrokeNative.dk_brush_kind_param(kind, 0),
                StrokeNative.dk_brush_kind_param(kind, 7),
                StrokeNative.dk_brush_kind_param(kind, 6),
                StrokeNative.dk_brush_kind_param(kind, 9),
                StrokeNative.dk_brush_kind_param(kind, 10),
                StrokeNative.dk_brush_param(_handle, 11),
                StrokeNative.dk_brush_param(_handle, 5),
                StrokeNative.dk_brush_param(_handle, 8),
                StrokeNative.dk_brush_kind_param(kind, 5),
                StrokeNative.dk_brush_kind_param(kind, 8),
                seedHalves[0], seedHalves[1]);
            _gpuStroke = true;
        }
        var pressureRadius = StrokeNative.dk_brush_param(_handle, 2);
        var painted = radius * (pressureRadius + (1f - pressureRadius) * pressure);
        var lean = MathF.Sqrt(tiltX * tiltX + tiltY * tiltY);
        var strength = Math.Clamp(lean <= 1f ? lean : lean / 60f, 0f, 1f);
        var direction = lean > 0.01f
            ? MathF.Atan2(tiltY, tiltX) * 180f / MathF.PI
            : azimuth * 180f / MathF.PI;
        var roundness = MathF.Max(
            StrokeNative.dk_brush_param(_handle, 7) * (1f - 0.85f * strength), 0.08f);
        var variants = new float[16];
        var count = StrokeNative.dk_mirror_segments(_handle, (float)x0, (float)y0,
            (float)x1, (float)y1, variants);
        if (count <= 0)
        {
            variants[0] = (float)x0;
            variants[1] = (float)y0;
            variants[2] = (float)x1;
            variants[3] = (float)y1;
            count = 1;
        }
        for (var index = 0; index < count; index++)
        {
            StrokeSurfaceNative.dk_canvas_surface_stroke_segment(_surface,
                variants[index * 4], variants[index * 4 + 1],
                variants[index * 4 + 2], variants[index * 4 + 3], painted,
                StrokeNative.dk_brush_param(_handle, 0), StrokeNative.dk_brush_param(_handle, 4),
                roundness, StrokeNative.dk_brush_param(_handle, 6) + direction, pressure);
        }
    }

    /// Composite the stroke into the resident tiles. False means the core refused this brush, so
    /// the caller replays the recorded stroke instead.
    public bool GpuStrokeFinish()
    {
        if (!_gpuStroke)
        {
            StrokeNative.dk_set_defer_raster(_handle, 0);
            return false;
        }
        _gpuStroke = false;
        if (StrokeNative.dk_stroke_tile_begin(_handle) == 0)
        {
            StrokeNative.dk_set_defer_raster(_handle, 0);
            return false;
        }
        var count = StrokeSurfaceNative.dk_canvas_surface_stroke_commit(_surface,
            StrokeNative.dk_layer_active(_handle),
            _gpuInk[0], _gpuInk[1], _gpuInk[2], StrokeNative.dk_brush_param(_handle, 1));
        var rects = new List<uint>();
        for (var index = 0; index < count; index++)
        {
            var rect = new uint[4];
            if (StrokeSurfaceNative.dk_canvas_surface_committed_rect(_surface, (uint)index, rect) != 0)
                rects.AddRange(rect);
        }
        StrokeSurfaceNative.dk_canvas_surface_stroke_end(_surface);
        StrokeNative.dk_stroke_gpu_end(_handle, rects.Count > 0 ? rects.ToArray() : null,
            (uint)(rects.Count / 4));
        StrokeNative.dk_set_defer_raster(_handle, 0);
        _gpuStale = true;
        PresentSurface();
        return true;
    }

    /// Rebuild the CPU bitmap from the resident tiles. Called before anything reads pixels.
    public void MaterializeCanvas()
    {
        if (!_gpuStale || _surface == IntPtr.Zero) return;
        _gpuStale = false;
        if (StrokeSurfaceNative.dk_canvas_surface_materialize(_surface) == 0) return;
        var rect = new uint[4];
        for (uint index = 0; ; index++)
        {
            var bytes = StrokeSurfaceNative.dk_canvas_surface_committed_info(_surface, index, rect);
            if (bytes <= 0) break;
            var layer = StrokeSurfaceNative.dk_canvas_surface_committed_layer(_surface, index);
            var pixels = new byte[bytes];
            if (StrokeSurfaceNative.dk_canvas_surface_committed_tile(_surface, index, rect,
                    pixels, (uint)bytes) == 0) continue;
            if (layer >= 0)
                StrokeNative.dk_stroke_apply_tiles(_handle, (uint)layer,
                    rect[0], rect[1], rect[2], rect[3], pixels);
        }
        StrokeSurfaceNative.dk_canvas_surface_clear_committed(_surface);
    }

    public bool UndoTiles()
    {
        if (_surface == IntPtr.Zero) return false;
        if (StrokeSurfaceNative.dk_canvas_surface_undo_tiles(_surface) == 0) return false;
        _gpuStale = true;
        PresentSurface();
        return true;
    }

    public bool RedoTiles()
    {
        if (_surface == IntPtr.Zero) return false;
        if (StrokeSurfaceNative.dk_canvas_surface_redo_tiles(_surface) == 0) return false;
        _gpuStale = true;
        PresentSurface();
        return true;
    }
}
