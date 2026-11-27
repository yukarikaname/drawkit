// File open and save. The pickers need the window handle the shell already owns, which is why
// this window is a Window and not a Page: WinRT.Interop.InitializeWithWindow takes the HWND.
// Ctrl+S reuses the last project path; the first save, Save As and any imported image ask.

using System;
using System.Collections.Generic;
using System.IO;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;

namespace DrawKit;

public sealed partial class MainWindow
{
    private string? _documentPath;

    /// Only a project or an EXR can be written back in place. An opened PNG is an import, and
    /// Ctrl+S on it must ask for a project name rather than overwrite it as a tape.
    private static bool IsProjectPath(string path)
    {
        var extension = Path.GetExtension(path);
        return extension.Equals(".dkp", StringComparison.OrdinalIgnoreCase) ||
               extension.Equals(".exr", StringComparison.OrdinalIgnoreCase);
    }

    private async void OpenClicked(object sender, RoutedEventArgs e)
    {
        if (_engine is null) return;
        var picker = new FileOpenPicker();
        foreach (var extension in new[] { ".dkp", ".kra", ".psd", ".png", ".jpg", ".jpeg" })
        {
            picker.FileTypeFilter.Add(extension);
        }
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is not null && _engine.Open(file.Path))
        {
            CanvasImage.Source = _engine.Bitmap;
            HdrToggle.IsOn = _engine.IsHdr;
            ExposureSlider.Value = _engine.Exposure;
            SnapToggle.IsOn = _engine.FillSnap;
            _documentPath = IsProjectPath(file.Path) ? file.Path : null;
            MarkSaved(file.Path);
            RefreshLayers();
        }
    }

    private async void SaveClicked(object sender, RoutedEventArgs e) => await SaveAsync(false);

    private async void SaveAsClicked(object sender, RoutedEventArgs e) => await SaveAsync(true);

    private async System.Threading.Tasks.Task SaveAsync(bool choosePath)
    {
        if (_engine is null) return;
        var path = _documentPath;
        if (choosePath || path is null || !IsProjectPath(path))
        {
            var picker = new FileSavePicker();
            picker.FileTypeChoices.Add("DrawKit project", new List<string> { ".dkp" });
            picker.FileTypeChoices.Add("OpenEXR", new List<string> { ".exr" });
            picker.SuggestedFileName = _documentName;
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null) return;
            path = file.Path;
        }
        var exr = Path.GetExtension(path).Equals(".exr", StringComparison.OrdinalIgnoreCase);
        var saved = exr ? _engine.SaveExr(path) : _engine.Save(path);
        if (!saved)
        {
            Status.Text = "Could not save";
            return;
        }
        _documentPath = path;
        MarkSaved(path);
        Status.Text = "Saved";
    }
}
