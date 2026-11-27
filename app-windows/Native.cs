// The Windows C ABI. Keep this a mirror of crates/core/src/ffi.rs; a missing entry fails at the
// first call, not at build time, so this file is deliberately small and checked against the Rust
// surface when the core changes.

using System;
using System.Runtime.InteropServices;

namespace DrawKit;

internal static class Native
{
    private const string Library = "drawkit_core";

    [DllImport(Library)]
    internal static extern int dk_text(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string key,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string language,
        byte[]? output, UIntPtr capacity);

    /// One label from `shared/strings.json`; the core owns the table, the shell owns the buffer.
    internal static string Text(string key, string language)
    {
        var length = dk_text(key, language, null, UIntPtr.Zero);
        if (length <= 0) return key;
        var buffer = new byte[length + 1];
        dk_text(key, language, buffer, (UIntPtr)buffer.Length);
        return System.Text.Encoding.UTF8.GetString(buffer, 0, length);
    }

    [DllImport(Library)] internal static extern IntPtr dk_editor_new(uint width, uint height);
    [DllImport(Library)] internal static extern void dk_editor_free(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_width(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_height(IntPtr handle);

    [DllImport(Library)] internal static extern void dk_set_stabilization(IntPtr handle, float strength);
    [DllImport(Library)] internal static extern void dk_set_auto_shape(IntPtr handle, int enabled);
    [DllImport(Library)] internal static extern int dk_auto_shape(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_pick(IntPtr handle, uint x, uint y, float[] outColor);
    [DllImport(Library)] internal static extern void dk_set_mirror(IntPtr handle, int mode);
    [DllImport(Library)] internal static extern void dk_set_mirror_angle(IntPtr handle, float degrees, int both);
    [DllImport(Library)] internal static extern void dk_set_eraser_strength(IntPtr handle, float strength);
    [DllImport(Library)] internal static extern void dk_set_fill_snap(IntPtr handle, int enabled);
    [DllImport(Library)] internal static extern int dk_fill_snap(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_color_space(IntPtr handle);

    [DllImport(Library)] internal static extern void dk_stroke_begin(IntPtr handle, ulong timeMs);
    [DllImport(Library)] internal static extern void dk_stroke_end(IntPtr handle);
    [DllImport(Library)]
    internal static extern void dk_stroke_ex(
        IntPtr handle, float x0, float y0, float x1, float y1,
        float radius, float pressure, float tiltX, float tiltY,
        float azimuth, float roll, float red, float green, float blue, float alpha,
        ulong timeMs);
    [DllImport(Library)]
    internal static extern void dk_erase_ex(
        IntPtr handle, float x0, float y0, float x1, float y1,
        float radius, float pressure, float tiltX, float tiltY,
        float azimuth, float roll, ulong timeMs);
    [DllImport(Library)]
    internal static extern int dk_extract_lineart(IntPtr handle, float threshold, float softness);
    [DllImport(Library)]
    internal static extern int dk_fill_begin(
        IntPtr handle, uint x, uint y, float tolerance,
        uint close, uint expand, uint feather,
        float red, float green, float blue, float alpha, ulong timeMs);
    [DllImport(Library)] internal static extern int dk_fill_step(IntPtr handle, float progress);
    [DllImport(Library)] internal static extern int dk_fill_finish(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_fill_cancel(IntPtr handle);
    [DllImport(Library)]
    internal static extern void dk_fill_ex(
        IntPtr handle, uint x, uint y, float tolerance,
        uint close, uint expand, uint feather,
        float red, float green, float blue, float alpha, ulong timeMs);

    [DllImport(Library)] internal static extern int dk_can_undo(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_can_redo(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_undo(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_redo(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_is_clear(IntPtr handle);
    [DllImport(Library)]
    internal static extern void dk_clear(
        IntPtr handle, float red, float green, float blue, float alpha);

    [DllImport(Library)] internal static extern int dk_composite(IntPtr handle, byte[] outPixels);
    [DllImport(Library)] internal static extern int dk_dynamic_range(IntPtr handle);
    [DllImport(Library)] internal static extern void dk_set_dynamic_range(IntPtr handle, int range);
    [DllImport(Library)] internal static extern int dk_composite_linear(IntPtr handle, float[] outPixels);
    [DllImport(Library)]
    internal static extern int dk_composite_linear_rect(
        IntPtr handle, uint x, uint y, uint width, uint height, float[] outPixels);
    [DllImport(Library)]
    internal static extern int dk_export_exr(
        IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string path);
    [DllImport(Library)]
    internal static extern int dk_composite_rect(
        IntPtr handle, uint x, uint y, uint width, uint height, byte[] outPixels);
    [DllImport(Library)] internal static extern int dk_take_dirty(IntPtr handle, uint[] outRect);

    [DllImport(Library)] internal static extern IntPtr dk_open_file([MarshalAs(UnmanagedType.LPUTF8Str)] string path);
    [DllImport(Library)]
    internal static extern int dk_save_tape(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

    [DllImport(Library)] internal static extern uint dk_layer_count(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_layer_active(IntPtr handle);
    [DllImport(Library)] internal static extern void dk_layer_select(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern uint dk_layer_add(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_layer_add_group(IntPtr handle);
    [DllImport(Library)]
    internal static extern uint dk_layer_add_fill(
        IntPtr handle, float red, float green, float blue, float alpha);
    [DllImport(Library)] internal static extern int dk_layer_remove(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_layer_move(IntPtr handle, uint index, int steps);
    [DllImport(Library)] internal static extern int dk_layer_set_name(
        IntPtr handle, uint index, [MarshalAs(UnmanagedType.LPUTF8Str)] string name);
    [DllImport(Library)] internal static extern int dk_layer_tag(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_tag(IntPtr handle, uint index, int tag);
    [DllImport(Library)] internal static extern int dk_layer_visible(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_visible(IntPtr handle, uint index, int visible);
    [DllImport(Library)] internal static extern float dk_layer_opacity(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_opacity(IntPtr handle, uint index, float opacity);
    [DllImport(Library)] internal static extern int dk_layer_kind(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern int dk_layer_parent(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern int dk_layer_expanded(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_expanded(IntPtr handle, uint index, int expanded);
    [DllImport(Library)]
    internal static extern int dk_layer_name(IntPtr handle, uint index, byte[] buffer, uint capacity);

    [DllImport(Library)]
    internal static extern int dk_layer_rect(
        IntPtr handle, uint index, uint x, uint y, uint width, uint height, byte[] outPixels);

    [DllImport(Library)] internal static extern int dk_layer_effective_visible(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern float dk_layer_effective_opacity(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern int dk_layer_blend(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_blend(IntPtr handle, uint index, int blend);
    [DllImport(Library)] internal static extern int dk_layer_locked(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern void dk_layer_set_locked(IntPtr handle, uint index, int locked);
    [DllImport(Library)] internal static extern int dk_layer_alpha_locked(IntPtr handle, uint index);
    [DllImport(Library)]
    internal static extern void dk_layer_set_alpha_locked(IntPtr handle, uint index, int locked);
    [DllImport(Library)] internal static extern int dk_layer_clipping(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern int dk_layer_clip_base(IntPtr handle, uint index);
    [DllImport(Library)] internal static extern int dk_layer_fill(IntPtr handle, uint index, float[] outColor);
    [DllImport(Library)]
    internal static extern int dk_layer_thumbnail(
        IntPtr handle, uint index, uint width, uint height, byte[] outPixels);
    [DllImport(Library)]
    internal static extern int dk_layer_add_image(
        IntPtr handle, uint width, uint height, byte[] pixels);

    [DllImport(Library)] internal static extern int dk_selection(IntPtr handle, uint[] outRect);
    [DllImport(Library)]
    internal static extern void dk_set_selection(
        IntPtr handle, uint x, uint y, uint width, uint height);
    [DllImport(Library)] internal static extern void dk_clear_selection(IntPtr handle);
    [DllImport(Library)] internal static extern int dk_extract_selection(IntPtr handle);

    [DllImport(Library)] internal static extern uint dk_frame_count(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_frame_active(IntPtr handle);
    [DllImport(Library)] internal static extern uint dk_fps(IntPtr handle);
}

/// The GPU layer compositor. The DLL is optional: a machine without a wgpu adapter falls back to
/// the core's CPU composite, and so does a machine where the DLL is missing.
internal static class GpuNative
{
    private const string Library = "drawkit_gpu";

    [DllImport(Library)] internal static extern IntPtr dk_gpu_compositor_new(uint width, uint height);
    [DllImport(Library)] internal static extern void dk_gpu_compositor_free(IntPtr handle);
    [DllImport(Library)]
    internal static extern void dk_gpu_compositor_reset(
        IntPtr handle, uint count, uint width, uint height);
    [DllImport(Library)]
    internal static extern void dk_gpu_compositor_set_budget(IntPtr handle, ulong bytes);
    [DllImport(Library)]
    internal static extern int dk_gpu_compositor_upload_layer(
        IntPtr handle, uint index, uint x, uint y, uint width, uint height, byte[] pixels);
    [DllImport(Library)]
    internal static extern void dk_gpu_compositor_set_layer(
        IntPtr handle, uint index, float opacity, int blend, int visible, int fill,
        float red, float green, float blue, float alpha);
    [DllImport(Library)]
    internal static extern void dk_gpu_compositor_set_layer_clip(
        IntPtr handle, uint index, int clip);
    [DllImport(Library)]
    internal static extern int dk_gpu_compositor_composite(
        IntPtr handle, byte[] outPixels, uint width, uint height);
}

/// The zero-copy swapchain surface. It takes the WinUI SwapChainPanel's
/// ISwapChainPanelNative pointer, not the managed panel object.
internal static class GpuSurfaceNative
{
    private const string Library = "drawkit_gpu";

    [DllImport(Library)]
    internal static extern IntPtr dk_canvas_surface_new_native(
        IntPtr display, ulong window, int platform, uint width, uint height, int hdr, int colorSpace);
    [DllImport(Library)] internal static extern void dk_canvas_surface_free(IntPtr handle);
    [DllImport(Library)]
    internal static extern void dk_canvas_surface_resize(IntPtr handle, uint width, uint height);
    [DllImport(Library)]
    internal static extern void dk_canvas_surface_reset_layers(
        IntPtr handle, uint count, uint canvasWidth, uint canvasHeight);
    [DllImport(Library)]
    internal static extern void dk_canvas_surface_set_layer_budget(IntPtr handle, ulong bytes);
    [DllImport(Library)]
    internal static extern int dk_canvas_surface_upload_layer(
        IntPtr handle, uint index, uint x, uint y, uint width, uint height, byte[] pixels);
    [DllImport(Library)]
    internal static extern void dk_canvas_surface_set_layer(
        IntPtr handle, uint index, float opacity, int blend, int visible, int fill,
        float red, float green, float blue, float alpha);
    [DllImport(Library)]
    internal static extern void dk_canvas_surface_set_layer_clip(
        IntPtr handle, uint index, int clip);
    [DllImport(Library)]
    internal static extern int dk_canvas_surface_present_layers(
        IntPtr handle, float paperX, float paperY, float paperWidth, float paperHeight);
    [DllImport(Library)]
    internal static extern int dk_canvas_surface_present_hdr(
        IntPtr handle, float[] pixels, uint canvasWidth, uint canvasHeight,
        float paperX, float paperY, float paperWidth, float paperHeight,
        uint dirtyX, uint dirtyY, uint dirtyWidth, uint dirtyHeight);
    [DllImport(Library)] internal static extern void dk_canvas_surface_set_exposure(
        IntPtr handle, float exposure);
    [DllImport(Library)] internal static extern int dk_canvas_surface_hdr(IntPtr handle);
}
