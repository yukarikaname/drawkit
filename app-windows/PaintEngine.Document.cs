// Document-level settings the shell mirrors in its own controls: the eraser strength, the fill's
// snap policy, the working colour space and the frame count. Each one is a single C call; the
// shell is not allowed to guess a value the core already owns.

namespace DrawKit;

internal sealed partial class PaintEngine
{
    public void SetEraserStrength(float strength) =>
        Native.dk_set_eraser_strength(_handle, strength);

    public void SetFillSnap(bool enabled) =>
        Native.dk_set_fill_snap(_handle, enabled ? 1 : 0);

    public bool FillSnap => Native.dk_fill_snap(_handle) != 0;

    /// 0 sRGB, 1 Display P3, 2 Adobe RGB; the same numbering `ColorSpace::as_i32` uses.
    public int ColourSpace => Native.dk_color_space(_handle);

    public int FrameCount => (int)Native.dk_frame_count(_handle);
    public int ActiveFrame => (int)Native.dk_frame_active(_handle);
    public int Fps => (int)Native.dk_fps(_handle);
}
