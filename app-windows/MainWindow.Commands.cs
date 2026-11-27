// The Edit menu without a menu bar: undo, the selection, and the shortcuts the macOS command
// surface binds. Keys are routed here from the root grid, so a bare letter only acts when the
// search field does not have the keyboard.

using System;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Windows.System;
using Windows.UI.Core;

namespace DrawKit;

public sealed partial class MainWindow
{
    private bool _typing;

    private static bool ControlDown =>
        (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control)
            & CoreVirtualKeyStates.Down) != 0;

    private static bool ShiftDown =>
        (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift)
            & CoreVirtualKeyStates.Down) != 0;

    private void UndoClicked(object sender, RoutedEventArgs e)
    {
        FinishFillAnimation();
        if (_engine?.Undo() != true) return;
        MarkDirty();
        RefreshLayers();
    }

    private void RedoClicked(object sender, RoutedEventArgs e)
    {
        FinishFillAnimation();
        if (_engine?.Redo() != true) return;
        MarkDirty();
        RefreshLayers();
    }

    private void SelectAll()
    {
        FinishFillAnimation();
        _engine?.SelectAll();
        UpdateStatus();
    }

    private void Deselect()
    {
        _engine?.ClearSelection();
        UpdateStatus();
    }

    private void ExtractSelection()
    {
        if (_engine?.ExtractSelection() != true) return;
        MarkDirty();
        RefreshLayers();
    }

    private void ClearLayer()
    {
        if (_engine is null) return;
        FinishFillAnimation();
        _engine.ClearLayer();
        MarkDirty();
        RefreshLayers();
    }

    // The flyout items need the plain handler shape; the work is the same as the key.
    private void SelectAllClicked(object sender, RoutedEventArgs e) => SelectAll();
    private void DeselectClicked(object sender, RoutedEventArgs e) => Deselect();
    private void ExtractSelectionClicked(object sender, RoutedEventArgs e) => ExtractSelection();
    private void ClearLayerClicked(object sender, RoutedEventArgs e) => ClearLayer();

    private void OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        // A text field keeps every key, including the accelerators that would otherwise fire.
        if (_typing) return;
        var control = ControlDown;
        var shift = ShiftDown;
        if (control)
        {
            var handled = true;
            switch (e.Key)
            {
                case VirtualKey.Z when shift:
                    RedoClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.Z:
                    UndoClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.Y:
                    RedoClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.S when shift:
                    SaveAsClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.S:
                    SaveClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.O:
                    OpenClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.A:
                    SelectAll();
                    break;
                case VirtualKey.D:
                    Deselect();
                    break;
                case VirtualKey.C when shift:
                    CopyMergedClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.C:
                    CopyClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.V:
                    PasteClicked(this, new RoutedEventArgs());
                    break;
                case VirtualKey.K:
                    ClearLayer();
                    break;
                case VirtualKey.J when shift:
                    ExtractSelection();
                    break;
                case VirtualKey.Add:
                case VirtualKey.OemPlus:
                    ZoomBy(1.25);
                    break;
                case VirtualKey.Subtract:
                case VirtualKey.OemMinus:
                    ZoomBy(1 / 1.25);
                    break;
                case VirtualKey.Number0:
                case VirtualKey.NumberPad0:
                    if (shift) FitClicked(this, new RoutedEventArgs());
                    else ActualSize();
                    break;
                default:
                    handled = false;
                    break;
            }
            if (handled)
            {
                e.Handled = true;
                return;
            }
        }

        switch (e.Key)
        {
            case VirtualKey.B:
                SelectTool("brush");
                break;
            case VirtualKey.E:
                SelectTool("eraser");
                break;
            case VirtualKey.F:
                SelectTool("fill");
                break;
            case VirtualKey.I:
                SelectTool("eyedropper");
                break;
            case VirtualKey.OemOpenBrackets:
                AdjustSize(-2);
                break;
            case VirtualKey.OemCloseBrackets:
                AdjustSize(2);
                break;
            case VirtualKey.Escape:
                Deselect();
                break;
            case VirtualKey.F2:
                RenameLayerClicked(this, new RoutedEventArgs());
                break;
            default:
                return;
        }
        e.Handled = true;
    }
}
