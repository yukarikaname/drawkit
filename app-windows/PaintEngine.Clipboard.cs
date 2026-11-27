// Pixels to and from the Windows clipboard. The core has no clipboard of its own: a copy reads
// the selection (or the whole layer) and an encoded PNG is what the rest of Windows understands;
// a paste decodes to premultiplied BGRA and lands as one new image layer.

using System;

namespace DrawKit;

internal sealed partial class PaintEngine
{
    /// Premultiplied BGRA8 for the clipboard, cropped to the selection when there is one.
    public bool CopyPixels(bool merged, out int width, out int height, out byte[] bgra)
    {
        bgra = Array.Empty<byte>();
        width = 0;
        height = 0;
        if (_handle == IntPtr.Zero) return false;
        MaterializeCanvas();

        var region = new uint[4];
        var x = 0;
        var y = 0;
        var cropWidth = Width;
        var cropHeight = Height;
        if (Native.dk_selection(_handle, region) != 0)
        {
            x = (int)Math.Min(region[0], (uint)Width);
            y = (int)Math.Min(region[1], (uint)Height);
            cropWidth = (int)Math.Min(region[2], (uint)(Width - x));
            cropHeight = (int)Math.Min(region[3], (uint)(Height - y));
        }
        if (cropWidth <= 0 || cropHeight <= 0) return false;

        var pixels = new byte[cropWidth * cropHeight * 4];
        var found = merged
            ? Native.dk_composite_rect(_handle, (uint)x, (uint)y,
                (uint)cropWidth, (uint)cropHeight, pixels) != 0
            : Native.dk_layer_rect(_handle, (uint)ActiveLayer, (uint)x, (uint)y,
                (uint)cropWidth, (uint)cropHeight, pixels) != 0;
        if (!found) return false;

        bgra = new byte[pixels.Length];
        for (var index = 0; index < pixels.Length; index += 4)
        {
            bgra[index] = pixels[index + 2];
            bgra[index + 1] = pixels[index + 1];
            bgra[index + 2] = pixels[index];
            bgra[index + 3] = pixels[index + 3];
        }
        width = cropWidth;
        height = cropHeight;
        return true;
    }

    /// Centre a pasted image on the canvas without scaling it, like the macOS shell.
    public bool PasteImage(int width, int height, byte[] bgra)
    {
        if (width <= 0 || height <= 0 || bgra.Length < width * height * 4) return false;
        var canvas = new byte[Width * Height * 4];
        var originX = (Width - width) / 2;
        var originY = (Height - height) / 2;
        for (var row = 0; row < height; row++)
        {
            var y = originY + row;
            if (y < 0 || y >= Height) continue;
            for (var column = 0; column < width; column++)
            {
                var x = originX + column;
                if (x < 0 || x >= Width) continue;
                var source = (row * width + column) * 4;
                var target = (y * Width + x) * 4;
                canvas[target] = bgra[source + 2];
                canvas[target + 1] = bgra[source + 1];
                canvas[target + 2] = bgra[source];
                canvas[target + 3] = bgra[source + 3];
            }
        }
        if (Native.dk_layer_add_image(_handle, (uint)Width, (uint)Height, canvas) == 0)
            return false;
        // The core names every image layer "Reference N"; a paste is not a reference.
        Native.dk_layer_set_name(_handle, (uint)ActiveLayer, "Pasted");
        FullComposite();
        return true;
    }
}
