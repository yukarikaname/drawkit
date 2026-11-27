// The status strip and the saved/unsaved flag. The flag lives in the shell because the core has
// no notion of "on disk": it knows pixels and undo steps, not files. Everything else here is a
// value the core owns, read once per update.

using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace DrawKit;

public sealed partial class MainWindow
{
    private static readonly string[] ColourSpaceNames = { "sRGB", "Display P3", "Adobe RGB" };

    private void UpdateStatus()
    {
        if (_engine is null) return;
        var hdr = _engine.IsHdr ? (_engine.HdrSurface ? " · HDR" : " · HDR (8-bit display)") : "";
        var space = _engine.ColourSpace;
        var spaceName = space >= 0 && space < ColourSpaceNames.Length
            ? ColourSpaceNames[space]
            : ColourSpaceNames[0];
        StatusCanvas.Text = $"{_engine.Width} × {_engine.Height}";
        StatusColour.Text = spaceName + hdr;
        var frames = _engine.FrameCount;
        if (frames > 1)
        {
            StatusFrames.Text = $"frame {_engine.ActiveFrame + 1}/{frames} · {_engine.Fps} fps";
            StatusFrames.Visibility = Visibility.Visible;
        }
        else
        {
            StatusFrames.Visibility = Visibility.Collapsed;
        }
        StatusDirty.Visibility = _dirty ? Visibility.Visible : Visibility.Collapsed;
        StatusSaved.Visibility = _dirty ? Visibility.Collapsed : Visibility.Visible;
        StatusZoom.Text = $"{_zoom * 100:0}%";
        Title = $"DrawKit — {_documentName}{(_dirty ? " •" : "")}";
        CanvasHint.Visibility = _engine.IsBlank ? Visibility.Visible : Visibility.Collapsed;
        ColourWellFill.Background = new SolidColorBrush(ColorPicker.Color);
    }

    private void MarkDirty()
    {
        _dirty = true;
        UpdateStatus();
    }

    private void MarkSaved(string path)
    {
        _dirty = false;
        _documentName = System.IO.Path.GetFileNameWithoutExtension(path);
        UpdateStatus();
    }

    private void ZoomInClicked(object sender, RoutedEventArgs e) => ZoomBy(1.25);

    private void ZoomOutClicked(object sender, RoutedEventArgs e) => ZoomBy(1 / 1.25);
}
