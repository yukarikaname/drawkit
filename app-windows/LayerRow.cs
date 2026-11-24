using Microsoft.UI.Xaml;

namespace DrawKit;

/// One row in the layer list. Rebuilt whenever the engine reports a layer change, so the bindings
/// stay one-way and small.
public sealed class LayerRow
{
    public LayerRow(LayerNode node, int depth)
    {
        Index = node.Index;
        Name = node.Name;
        Visible = node.Visible;
        Opacity = node.Opacity;
        Kind = node.Kind;
        Depth = depth;
        Expanded = node.Expanded;
    }

    public uint Index { get; }
    public string Name { get; }
    public bool Visible { get; }
    public double Opacity { get; }
    public int Kind { get; }
    public int Depth { get; }
    public bool Expanded { get; }

    public bool IsGroup => Kind == 1;
    public string Glyph => Kind == 1 ? "\uE8B7" : Kind == 2 ? "\uE790" : "\uE70F";
    public string OpacityText => $"{Opacity * 100:0}%";
    public Thickness Indent => new(Depth * 14, 0, 0, 0);
}
