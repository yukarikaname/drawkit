// Copy merged and paste through the Windows clipboard. The core has no clipboard, so a copy is
// encoded as PNG (what every Windows app understands) and a paste decodes to premultiplied BGRA
// and lands as one new layer. The clipboard is a shared resource: failures are reported, not
// thrown, because another process can hold it open.

using System;
using System.Threading.Tasks;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;
using System.Runtime.InteropServices.WindowsRuntime;

namespace DrawKit;

public sealed partial class MainWindow
{
    private async void CopyClicked(object sender, RoutedEventArgs e) => await CopyAsync(false);

    private async void CopyMergedClicked(object sender, RoutedEventArgs e) => await CopyAsync(true);

    private async void PasteClicked(object sender, RoutedEventArgs e) => await PasteAsync();

    private async Task CopyAsync(bool merged)
    {
        if (_engine is null) return;
        if (!_engine.CopyPixels(merged, out var width, out var height, out var bgra))
        {
            Status.Text = "Nothing to copy";
            return;
        }
        try
        {
            using var stream = new InMemoryRandomAccessStream();
            var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
            encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
                (uint)width, (uint)height, 96, 96, bgra);
            await encoder.FlushAsync();
            stream.Seek(0);
            var package = new DataPackage();
            package.SetBitmap(RandomAccessStreamReference.CreateFromStream(stream));
            Clipboard.SetContent(package);
            // Flush hands the data to the OS, so the stream can go away with this method.
            Clipboard.Flush();
            Status.Text = merged ? "Copied merged" : "Copied layer";
        }
        catch (Exception)
        {
            Status.Text = "Could not reach the clipboard";
        }
    }

    private async Task PasteAsync()
    {
        if (_engine is null) return;
        try
        {
            var content = Clipboard.GetContent();
            if (!content.Contains(StandardDataFormats.Bitmap))
            {
                Status.Text = "The clipboard has no image";
                return;
            }
            var reference = await content.GetBitmapAsync();
            using var stream = await reference.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(stream);
            using var software = await decoder.GetSoftwareBitmapAsync(
                BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied);
            var width = (int)software.PixelWidth;
            var height = (int)software.PixelHeight;
            var bgra = new byte[width * height * 4];
            software.CopyToBuffer(bgra.AsBuffer());
            if (!_engine.PasteImage(width, height, bgra))
            {
                Status.Text = "Could not paste";
                return;
            }
            MarkDirty();
            RefreshLayers();
            Status.Text = "Pasted";
        }
        catch (Exception)
        {
            Status.Text = "The clipboard has no image";
        }
    }
}
