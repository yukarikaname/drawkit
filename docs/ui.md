# DrawKit UI contract

Two product classes, two interaction contracts. Shells inside a class should read the same and
use the same words and order; the platform supplies the chrome, the gestures and the file picker.

## Shared tokens

Ink `#1B1917`, carbon `#24211E`, raised `#2E2A26`, paper `#F4EFE7`, muted `#A69E93`,
vermilion `#C5523D`. Tool targets are at least 40 pt/dp. Type is a 12 pt label, a 10 pt value and
a 13 pt status; headings are small caps.

## Desktop (macOS, Windows)

A desktop is a precision instrument: pointer, keyboard, docked panels, menus and persisted
window state.

- Top command strip: brand, file open/save, undo/redo, symmetry, view. The same order on both.
- Left tool rail: brush, eraser, fill; selected tool is solid vermilion.
- Right inspector, docked: colour, brush, fill, layers. It does not cover the canvas.
- Layers stay visible while painting; selection, visibility, opacity and grouping are one click.
- Keyboard: B/E/F tools, Ctrl/Cmd+Z/Y undo/redo, Ctrl/Cmd+S/O save/open.
- Mirror is a free axis: angle in degrees, optional perpendicular axis. Not H/V only.
- Status sits at the canvas foot: document size, tool, layer count.
- Native chrome: macOS menu bar and sheets; Windows title bar, WinUI flyouts and Windows Ink.

## Tablet (iPadOS, Android tablet)

A tablet is a hand-held sheet: pen, touch, floating chrome and sheets that can be dismissed.

- Floating top capsule: brand, brush/eraser/fill, undo/redo, layers, overflow. It does not span
  the full width.
- Floating tool card at the left edge: Size, Opacity, Mirror, Steady. Fill swaps to Tolerance,
  Gap, Grow, Soft. The order is fixed across tablets.
- Bottom-left status, bottom-right colour well.
- Layers and colours open as sheets; the canvas is never permanently reduced by a docked panel.
- Targets are 44 pt/dp; labels stay at 11–12 pt. Sheets use the platform's detents and drag
  handle.
- Mirror is the touch-friendly H/V/Both set; the free-angle axis is a desktop affordance.
- Platform: Apple Pencil pressure/tilt/roll and two-finger undo on iPad; Android stylus pressure
  and back-gesture dismissal on Android.

## The 70% rule

Within a class, operations, labels, order and state names match. What stays platform-specific:
window chrome, menus, file pickers, keyboard shortcuts, touch gestures, sheet presentation and
the shape of docked panels. Never copy a desktop dock onto a tablet or a tablet sheet onto a
desktop.
