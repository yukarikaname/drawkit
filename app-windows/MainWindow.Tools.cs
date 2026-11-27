// The tool options bar. CSP changes this row with the tool, and so does this shell: a setting
// that does not apply is collapsed, not shown disabled. The brush and the eraser keep their own
// size and opacity, because a 3 px liner and a 60 px eraser are used in the same minute.

using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;

namespace DrawKit;

public sealed partial class MainWindow
{
    private void ToolClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not ToggleButton button || button.Tag is not string tool) return;
        SelectTool(tool);
    }

    private void SelectTool(string tool)
    {
        _tool = tool;
        SyncToolButtons();
        SyncToolOptions();
        UpdateStatus();
    }

    private void SyncToolButtons()
    {
        BrushButton.IsChecked = _tool == "brush";
        EraserButton.IsChecked = _tool == "eraser";
        FillButton.IsChecked = _tool == "fill";
        EyedropperButton.IsChecked = _tool == "eyedropper";
        RailBrushButton.IsChecked = _tool == "brush";
        RailEraserButton.IsChecked = _tool == "eraser";
        RailFillButton.IsChecked = _tool == "fill";
        RailEyedropperButton.IsChecked = _tool == "eyedropper";
    }

    private void SyncToolOptions()
    {
        var brush = _tool == "brush";
        var eraser = _tool == "eraser";
        var fill = _tool == "fill";
        BrushOptions.Visibility = brush ? Visibility.Visible : Visibility.Collapsed;
        EraserOptions.Visibility = eraser ? Visibility.Visible : Visibility.Collapsed;
        FillOptions.Visibility = fill ? Visibility.Visible : Visibility.Collapsed;
        PickerOptions.Visibility = _tool == "eyedropper" ? Visibility.Visible : Visibility.Collapsed;
        AssistOptions.Visibility = brush || eraser ? Visibility.Visible : Visibility.Collapsed;
        MirrorOptions.Visibility = brush || eraser ? Visibility.Visible : Visibility.Collapsed;
    }

    /// Half the slider value: the core takes a radius, the painter thinks in a width.
    private float ActiveRadius =>
        (float)(_tool == "eraser" ? EraserSizeSlider.Value : SizeSlider.Value) / 2f;

    private void AdjustSize(double steps)
    {
        if (_tool == "eraser")
        {
            EraserSizeSlider.Value = Math.Clamp(EraserSizeSlider.Value + steps,
                EraserSizeSlider.Minimum, EraserSizeSlider.Maximum);
        }
        else
        {
            SizeSlider.Value = Math.Clamp(SizeSlider.Value + steps,
                SizeSlider.Minimum, SizeSlider.Maximum);
        }
    }

    private void BrushSizeChanged(object sender, RangeBaseValueChangedEventArgs e) =>
        UpdateToolReadouts();

    private void EraserSizeChanged(object sender, RangeBaseValueChangedEventArgs e) =>
        UpdateToolReadouts();

    private void EraserOpacityChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        _engine?.SetEraserStrength((float)e.NewValue);
        UpdateToolReadouts();
    }

    private void OpacityChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        ApplyColor();
        UpdateToolReadouts();
    }

    private void SteadyChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        _engine?.SetStabilization((float)e.NewValue);
        UpdateToolReadouts();
    }

    private void AutoShapeToggled(object sender, RoutedEventArgs e) =>
        _engine?.SetAutoShape(AutoShapeToggle.IsOn);

    private void SnapToggled(object sender, RoutedEventArgs e) =>
        _engine?.SetFillSnap(SnapToggle.IsOn);

    private void FillOptionChanged(object sender, RangeBaseValueChangedEventArgs e) =>
        UpdateToolReadouts();

    private void MirrorToggled(object sender, RoutedEventArgs e)
    {
        ApplyMirror();
        UpdateToolReadouts();
    }

    private void MirrorAngleChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        ApplyMirror();
        UpdateToolReadouts();
    }

    /// The strip button is a shortcut for the switch in the bar, not a second state.
    private void MirrorClicked(object sender, RoutedEventArgs e)
    {
        MirrorToggle.IsOn = MirrorButton.IsChecked == true;
    }

    private void ApplyMirror()
    {
        if (_engine is null || MirrorToggle is null || MirrorAngleSlider is null ||
            MirrorBothToggle is null) return;
        if (MirrorToggle.IsOn)
            _engine.SetMirrorAngle((float)MirrorAngleSlider.Value, MirrorBothToggle.IsOn);
        else
            _engine.SetMirror(0);
    }

    private void UpdateToolReadouts()
    {
        // XAML sets the sliders' initial values while it is still building the tree, so the
        // handlers can run before the readout fields exist.
        if (SizeReadout is null) return;
        SizeReadout.Text = $"{SizeSlider.Value:0} px";
        OpacityReadout.Text = $"{OpacitySlider.Value * 100:0}%";
        EraserSizeReadout.Text = $"{EraserSizeSlider.Value:0} px";
        EraserOpacityReadout.Text = $"{EraserOpacitySlider.Value * 100:0}%";
        SmoothReadout.Text = $"{SteadySlider.Value:0.0}";
        ToleranceReadout.Text = $"{ToleranceSlider.Value * 100:0}%";
        GapReadout.Text = $"{GapSlider.Value:0}";
        GrowReadout.Text = $"{GrowSlider.Value:0}";
        SoftReadout.Text = $"{SoftSlider.Value:0}";
        MirrorAngleReadout.Text = $"{MirrorAngleSlider.Value:0}°";
        MirrorButton.IsChecked = MirrorToggle.IsOn;
    }
}
