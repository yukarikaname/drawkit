using System;
using System.Diagnostics;
using Microsoft.UI.Xaml;

namespace DrawKit;

// The spreading fill: the core writes the prepared mask in rings from the tap, this timer
// advances the reveal and commits one undo step at the end.
public sealed partial class MainWindow
{
    private DispatcherTimer? _fillTimer;
    private readonly Stopwatch _fillClock = new();

    private bool BeginFill(int x, int y)
    {
        FinishFillAnimation();
        if (_engine is null) return false;
        var started = _engine.BeginFill(x, y, (float)ToleranceSlider.Value,
            (int)GapSlider.Value, (int)GrowSlider.Value, (int)SoftSlider.Value, _color);
        if (!started) return false;
        _fillClock.Restart();
        _fillTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(16) };
        _fillTimer.Tick += FillTick;
        _fillTimer.Start();
        return true;
    }

    private void FillTick(object? sender, object e)
    {
        if (_engine is null)
        {
            FinishFillAnimation();
            return;
        }
        var progress = Math.Clamp(_fillClock.Elapsed.TotalSeconds / 0.22, 0, 1);
        var eased = 1 - Math.Pow(1 - progress, 3);
        _engine.FillStep((float)eased);
        if (progress >= 1) FinishFillAnimation();
    }

    private void FinishFillAnimation()
    {
        if (_fillTimer is null) return;
        _fillTimer.Stop();
        _fillTimer.Tick -= FillTick;
        _fillTimer = null;
        _engine?.FinishFill();
        MarkDirty();
        RefreshLayers();
    }

    private void CancelFillAnimation()
    {
        if (_fillTimer is null) return;
        _fillTimer.Stop();
        _fillTimer.Tick -= FillTick;
        _fillTimer = null;
        _engine?.CancelFill();
    }
}
