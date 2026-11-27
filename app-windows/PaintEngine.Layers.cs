// Layer state the palette edits: blend, lock, alpha lock, duplicate, thumbnails, and the
// selection rectangle the clipboard and the Edit menu work on. Everything here goes through the
// same FFI the rest of the engine uses, so undo behaves like any other layer edit.

namespace DrawKit;

internal sealed partial class PaintEngine
{
    public void SetBlend(int index, int blend)
    {
        Native.dk_layer_set_blend(_handle, (uint)index, blend);
        // The resident compositors flatten a plain group and refuse a parented blend. A group
        // that gains a blend has to fall back to the core's CPU path instead of presenting the
        // old flattening.
        var parent = Native.dk_layer_parent(_handle, (uint)index);
        if (Native.dk_layer_kind(_handle, (uint)index) == 1 || (parent >= 0 && blend != 0))
            InvalidateResident();
        RefreshSpecs();
    }

    private void InvalidateResident()
    {
        _surfaceReady = false;
        _gpuReady = false;
    }

    /// A locked layer refuses strokes in the core; the shell only has to stop painting.
    public void SetLocked(int index, bool locked) =>
        Native.dk_layer_set_locked(_handle, (uint)index, locked ? 1 : 0);

    public void SetAlphaLocked(int index, bool locked) =>
        Native.dk_layer_set_alpha_locked(_handle, (uint)index, locked ? 1 : 0);

    /// A duplicate is a new image layer from the source pixels, then the same opacity and blend.
    /// Groups have no pixels of their own, so they are not duplicated.
    public bool DuplicateLayer()
    {
        var source = ActiveLayer;
        if (Native.dk_layer_kind(_handle, (uint)source) != 0) return false;
        var pixels = new byte[Width * Height * 4];
        if (Native.dk_layer_rect(_handle, (uint)source, 0, 0,
                (uint)Width, (uint)Height, pixels) == 0) return false;
        var name = new byte[256];
        var written = Native.dk_layer_name(_handle, (uint)source, name, (uint)name.Length);
        var label = written > 0
            ? System.Text.Encoding.UTF8.GetString(name, 0, written)
            : $"Layer {source + 1}";
        if (Native.dk_layer_add_image(_handle, (uint)Width, (uint)Height, pixels) == 0)
            return false;
        var copy = ActiveLayer;
        Native.dk_layer_set_name(_handle, (uint)copy, label + " copy");
        Native.dk_layer_set_opacity(_handle, (uint)copy,
            Native.dk_layer_opacity(_handle, (uint)source));
        Native.dk_layer_set_blend(_handle, (uint)copy,
            Native.dk_layer_blend(_handle, (uint)source));
        FullComposite();
        return true;
    }

    /// Premultiplied RGBA8 at `width` x `height`, or null when the layer has no pixels.
    public byte[]? Thumbnail(int index, int width, int height)
    {
        if (width <= 0 || height <= 0) return null;
        var pixels = new byte[width * height * 4];
        return Native.dk_layer_thumbnail(_handle, (uint)index, (uint)width, (uint)height, pixels) != 0
            ? pixels
            : null;
    }

    public void SelectAll() =>
        Native.dk_set_selection(_handle, 0, 0, (uint)Width, (uint)Height);

    public void ClearSelection() => Native.dk_clear_selection(_handle);

    public bool ExtractSelection()
    {
        if (Native.dk_extract_selection(_handle) == 0) return false;
        FullComposite();
        return true;
    }

    public void ClearLayer()
    {
        Native.dk_clear(_handle, 0, 0, 0, 0);
        FullComposite();
    }
}
