namespace DrawKit;

/// One layer as the core reports it. The shell copies this into a row and never writes through
/// it, so a stale row can only show stale pixels, not corrupt the stack.
public sealed record LayerNode(
    uint Index, string Name, bool Visible, double Opacity,
    int Kind, int? Parent, bool Expanded, int Tag,
    int Blend, bool Locked, bool AlphaLocked);
