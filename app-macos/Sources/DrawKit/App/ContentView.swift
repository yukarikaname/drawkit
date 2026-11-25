// The window layout, after Clip Studio Paint: a narrow tool rail on the left, the canvas in the
// middle, docked palettes on the right, and tool properties directly above the canvas. The title
// bar carries the file and edit actions that a macOS app is expected to put there.

import SwiftUI
import AppKit
import UniformTypeIdentifiers

struct CanvasRepresentable: NSViewRepresentable {
    let model: PaintModel
    let figure: FigureModel
    let tool: Tool
    let size: Double
    let smudgeStrength: Double
    let zoom: Double
    let fitRequest: Int
    let rotationRequest: Int
    let rotation: Double
    let navigateX: Double
    let navigateY: Double
    let navigateRequest: Int
    let panX: Double
    let panY: Double
    let shortcuts: ShortcutStore
    let revision: UInt64
    let tolerance: Double
    let fillClose: Double
    let fillExpand: Double
    let fillFeather: Double
    let mirrorEnabled: Bool
    let mirrorAngle: Double
    let mirrorBoth: Bool
    let colorSpace: PaintColorSpace
    let extended: Bool
    let exposure: Double
    let pressureCurve: Double
    let pressureLow: Double
    let pressureHigh: Double
    let ink: Ink
    let tabletPrimary: TabletButtonAction
    let tabletSecondary: TabletButtonAction
    let onTabletDetected: () -> Void
    let recentColours: [Color]
    let currentBrush: BrushKind
    let onBrush: (BrushKind) -> Void
    let onColor: (Color) -> Void
    let onTool: (Tool) -> Void
    let onSize: (Double) -> Void
    let onPick: (Color, Double) -> Void
    let onZoom: (Double) -> Void
    let onPan: (CGPoint) -> Void
    let onViewport: (CGRect) -> Void
    let onRotation: (CGFloat) -> Void

    func makeNSView(context: Context) -> CanvasView {
        let view = CanvasView()
        view.model = model
        view.shortcuts = shortcuts
        view.onTool = onTool
        view.onSize = { onSize(Double($0)) }
        view.onPick = onPick
        view.onZoom = { onZoom(Double($0)) }
        view.onPan = onPan
        view.onViewport = onViewport
        view.colorSpace = colorSpace
        view.tabletPrimary = tabletPrimary
        view.tabletSecondary = tabletSecondary
        view.onTabletDetected = onTabletDetected
        view.recentColours = recentColours
        view.currentBrush = currentBrush
        view.onBrush = onBrush
        view.onColor = onColor
        view.onRotation = { radians in
            onRotation(radians)
        }
        return view
    }

    func updateNSView(_ view: CanvasView, context: Context) {
        view.model = model
        view.refreshGPUAvailability()
        view.shortcuts = shortcuts
        view.tool = tool
        view.ink = ink
        view.radius = CGFloat(size)
        view.smudgeStrength = smudgeStrength
        view.tolerance = Float(tolerance)
        view.fillClose = Int(fillClose)
        view.fillExpand = Int(fillExpand)
        view.fillFeather = Int(fillFeather)
        view.mirrorEnabled = mirrorEnabled
        view.mirrorAngle = mirrorAngle
        view.mirrorBoth = mirrorBoth
        view.onionImage = model.onionImage
        view.setColorSpace(colorSpace)
        view.setExtended(extended)
        view.setExposure(exposure)
        view.pressureCurve = pressureCurve
        view.pressureLow = pressureLow
        view.pressureHigh = pressureHigh
        view.tabletPrimary = tabletPrimary
        view.tabletSecondary = tabletSecondary
        view.onTabletDetected = onTabletDetected
        view.recentColours = recentColours
        view.currentBrush = currentBrush
        view.onBrush = onBrush
        view.onColor = onColor
        view.setExternalZoom(CGFloat(zoom))
        view.setExternalRotation(CGFloat(rotation))
        view.setExternalPan(CGPoint(x: panX, y: panY))
        if view.fitRequest != fitRequest {
            view.fitRequest = fitRequest
            view.fit()
        }
        if view.rotationRequest != rotationRequest {
            view.rotationRequest = rotationRequest
            view.resetRotation()
        }
        if view.navigateRequest != navigateRequest {
            view.navigateRequest = navigateRequest
            view.center(onNormalized: CGPoint(x: navigateX, y: navigateY))
        }
        view.presentSurface()
        view.reportViewport()
    }
}

struct ContentView: View {
    let model: PaintModel
    let figure: FigureModel
    let shortcuts: ShortcutStore
    @Bindable var workspace: Workspace
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @Environment(AppSettings.self) private var settings
    @State private var showWelcome = !CanvasView.benchmarkRequested
    @State private var recovery: AutosaveRecord?
    @State private var restoredSession = false
    @State private var showTabletTip = false
    @State private var dropTargeted = false
    @State private var showBranches = false
    @State private var showReplay = false
    @AppStorage("drawkit.experimental.historyBranches")
    private var historyBranches = false

    var body: some View {
        ZStack {
            if showWelcome {
                WelcomeView(model: model,
                            onCreate: { width, height, colorSpace, hdr in
                                model.reset(width: width, height: height,
                                            colorSpace: colorSpace, hdr: hdr)
                                showWelcome = false
                            },
                            onOpened: { showWelcome = false })
                    .transition(.opacity)
            } else {
                editor
            }
            if !settings.hasCompletedOnboarding {
                OnboardingView(shortcuts: shortcuts) {
                    settings.hasCompletedOnboarding = true
                }
                .transition(.opacity)
            }
        }
        .frame(minWidth: 1180, minHeight: 720)
        .overlay {
            if dropTargeted {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(Brand.vermilion, lineWidth: 2)
                    .padding(8)
                    .allowsHitTesting(false)
            }
        }
        .onDrop(of: [UTType.fileURL], isTargeted: $dropTargeted) { acceptDrop($0) }
        .preferredColorScheme(.dark)
        .onChange(of: model.documentURL) { _, url in
            if url != nil { showWelcome = false }
        }
        .onChange(of: workspace.stabilization) { _, value in
            model.setStabilization(value / 10 * 0.9)
        }
        .onChange(of: workspace.mirrorEnabled) { _, _ in applyMirror(model, workspace) }
        .onChange(of: workspace.mirrorAngle) { _, _ in applyMirror(model, workspace) }
        .onChange(of: workspace.mirrorBoth) { _, _ in applyMirror(model, workspace) }
        .onChange(of: workspace.brush) { _, value in
            if model.brushKind != value {
                model.setBrush(value)
            }
        }
        .onChange(of: workspace.fillSnap) { _, value in
            model.setFillSnap(value)
        }
        .onChange(of: historyBranches) { _, value in
            model.setHistoryBranches(value)
        }
        .onChange(of: workspace.strokeDedupe) { _, value in
            model.setStrokeDedupe(value)
        }
        .onChange(of: workspace.prediction) { _, value in
            model.setPrediction(value)
        }
        .onChange(of: workspace.autoShape) { _, value in
            model.setAutoShape(value)
        }
        .onChange(of: workspace.quickShape) { _, value in
            model.setQuickShape(value)
        }
        .onAppear {
            model.setHistoryBranches(historyBranches)
            model.setFillSnap(workspace.fillSnap)
            model.setStrokeDedupe(workspace.strokeDedupe)
            model.setPrediction(workspace.prediction)
            model.setAutoShape(workspace.autoShape)
            model.setQuickShape(workspace.quickShape)
        }
        .onChange(of: workspace.tool) { _, value in
        }
        .navigationTitle(model.documentURL?.deletingPathExtension().lastPathComponent
                         ?? "Untitled")
        .onChange(of: model.documentURL) { _, _ in updateWindowChrome() }
        .onChange(of: model.isDirty) { _, _ in updateWindowChrome() }
        .onAppear {
            updateWindowChrome()
            restoreWindowFrame()
            recovery = Autosave.pending()
            restoreSessionIfPossible()
        }
        .alert("Tablet mapping", isPresented: $showTabletTip) {
            Button("Got it") { settings.hasSeenTabletTip = true }
            Button("Remind me later", role: .cancel) {}
        } message: {
            Text("In your tablet driver, enable Force Proportions (强制等比例) and match the tablet area to the screen's aspect ratio. Without it, circles draw as ellipses.")
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            Session.save(model: model, workspace: workspace)
        }
        .task(id: settings.autosaveInterval) {
            guard settings.autosaveInterval > 0 else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(settings.autosaveInterval))
                if Task.isCancelled { break }
                model.autosaveIfNeeded()
            }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .inactive || phase == .background {
                model.autosaveIfNeeded()
                Session.save(model: model, workspace: workspace)
            }
        }
        .onChange(of: model.documentURL) { _, _ in
            Session.save(model: model, workspace: workspace)
        }
        .alert(item: $recovery) { record in
            Alert(
                title: Text("Recover painting?"),
                message: Text("A recovery copy of \(record.sourceName) from \(record.savedAt.formatted(date: .abbreviated, time: .shortened)) is newer than the last save."),
                primaryButton: .default(Text("Recover")) {
                    model.recover(record)
                    showWelcome = false
                },
                secondaryButton: .destructive(Text("Discard")) {
                    recovery = nil
                    model.discardAutosave()
                    // The recovery copy is gone; fall back to the last saved project the session
                    // pointed at instead of leaving the person on a blank sheet.
                    restoreSessionIfPossible()
                }
            )
        }
        .animation(Motion.panel.resolving(reduceMotion: reduceMotion), value: model.recordingMs > 0)
        .toolbar {
            if !showWelcome && settings.hasCompletedOnboarding {
                toolbar
            }
        }
    }

    private func updateWindowChrome() {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.canBecomeMain })
        else { return }
        window.isDocumentEdited = model.isDirty
        window.representedURL = model.documentURL
    }

    private func restoreWindowFrame() {
        guard let window = NSApp.windows.first(where: { $0.isVisible }) else { return }
        guard window.frameAutosaveName.isEmpty else { return }
        let name = NSWindow.FrameAutosaveName("DrawKitMainWindow")
        window.setFrameAutosaveName(name)
        _ = window.setFrameUsingName(name)
    }

    private func acceptDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.fileURL.identifier) {
            data, _ in
            guard let data, let url = URL(dataRepresentation: data, relativeTo: nil)
            else { return }
            DispatchQueue.main.async {
                showWelcome = false
                _ = model.openDocument(url)
            }
        }
        return true
    }

    private func applyMirror(_ model: PaintModel, _ workspace: Workspace) {
        model.setMirror(enabled: workspace.mirrorEnabled,
                        angle: workspace.mirrorAngle,
                        both: workspace.mirrorBoth)
    }

    private var editor: some View {
        ZStack {
            LinearGradient(colors: [Brand.ink, Brand.carbon],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
            VStack(spacing: 8) {
                ToolOptionsBar(tool: workspace.tool, model: model, shortcuts: shortcuts,
                               onImportBrush: {
                                   if let kind = model.importBrush() {
                                       workspace.brush = kind
                                   }
                               },
                               size: $workspace.size,
                               smudgeStrength: $workspace.smudgeStrength,
                               stabilization: $workspace.stabilization,
                               tolerance: $workspace.tolerance,
                               fillClose: $workspace.fillClose,
                               fillExpand: $workspace.fillExpand,
                               fillFeather: $workspace.fillFeather,
                               fillSnap: $workspace.fillSnap,
                               strokeDedupe: $workspace.strokeDedupe,
                               prediction: $workspace.prediction, autoShape: $workspace.autoShape,
                               quickShape: $workspace.quickShape,
                               mirrorEnabled: $workspace.mirrorEnabled,
                               mirrorAngle: $workspace.mirrorAngle,
                               mirrorBoth: $workspace.mirrorBoth,
                               hdr: model.hdr, glow: $workspace.glow,
                               exposure: $workspace.exposure,
                               brush: $workspace.brush)
                HStack(spacing: 10) {
                    ToolRail(tool: $workspace.tool, shortcuts: shortcuts)
                    ZStack {
                        CanvasRepresentable(
                            model: model, figure: figure, tool: workspace.tool,
                            size: workspace.size, smudgeStrength: workspace.smudgeStrength,
                            zoom: workspace.zoom,
                            fitRequest: workspace.fitRequest,
                            rotationRequest: workspace.rotationRequest,
                            rotation: workspace.rotation,
                            navigateX: workspace.navigateX, navigateY: workspace.navigateY,
                            navigateRequest: workspace.navigateRequest,
                            panX: workspace.panX, panY: workspace.panY, shortcuts: shortcuts,
                            revision: model.revision &+ figure.revision,
                            tolerance: workspace.tolerance,
                            fillClose: workspace.fillClose,
                            fillExpand: workspace.fillExpand,
                            fillFeather: workspace.fillFeather,
                            mirrorEnabled: workspace.mirrorEnabled,
                            mirrorAngle: workspace.mirrorAngle,
                            mirrorBoth: workspace.mirrorBoth,
                            colorSpace: model.colorSpace,
                            extended: model.hdr || model.colorSpace == .adobeRgb,
                            exposure: model.hdr ? workspace.exposure : 0,
                            pressureCurve: settings.pressureCurve,
                            pressureLow: settings.pressureLow,
                            pressureHigh: settings.pressureHigh,
                            ink: Ink.from(workspace.color, space: model.colorSpace,
                                          glow: model.hdr ? workspace.glow : 0),
                            tabletPrimary: settings.tabletPrimary,
                            tabletSecondary: settings.tabletSecondary,
                            onTabletDetected: {
                                if !settings.hasSeenTabletTip { showTabletTip = true }
                            },
                            recentColours: workspace.recentColours,
                            currentBrush: workspace.brush,
                            onBrush: { workspace.brush = $0 },
                            onColor: { workspace.color = $0 },
                            onTool: { workspace.tool = $0 },
                            onSize: { workspace.size = $0 },
                            onPick: { color, glow in
                                workspace.color = color
                                if model.hdr { workspace.glow = glow }
                            },
                            onZoom: { workspace.zoom = $0 },
                            onPan: { workspace.panX = $0.x; workspace.panY = $0.y },
                            onViewport: { workspace.viewport = $0 },
                            onRotation: { workspace.rotation = Double($0) * 180 / .pi }
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 12, style: .continuous)
                                .stroke(.white.opacity(0.06), lineWidth: 1)
                                .allowsHitTesting(false)
                        )
                        NavigatorView(model: model, viewport: workspace.viewport) { point in
                            workspace.navigateX = point.x
                            workspace.navigateY = point.y
                            workspace.navigateRequest &+= 1
                        }
                            .frame(maxWidth: .infinity, maxHeight: .infinity,
                                   alignment: .topTrailing)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .animation(Motion.settle.resolving(reduceMotion: reduceMotion),
                               value: model.isBlank)
                    SidePanel(model: model, color: $workspace.color,
                              recent: $workspace.recentColours)
                }
                StatusBar(width: model.width, height: model.height,
                          colorSpace: model.colorSpace, hdr: model.hdr,
                          frameCount: model.frameCount, activeFrame: model.activeFrame,
                          fps: model.fps, dirty: model.isDirty, status: model.status,
                          zoom: $workspace.zoom, rotation: workspace.rotation,
                          onResetRotation: { workspace.rotationRequest &+= 1 })
            }
            .padding(.horizontal, 10)
            .padding(.top, 8)
            .padding(.bottom, 4)
        }
        .background(Brand.ink)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .navigation) {
            Button { model.openProject() } label: { Image(systemName: "folder") }
                .help("Open project or image (\(shortcuts.display(.open)))")
            Button { model.saveProject() } label: { Image(systemName: "square.and.arrow.down") }
                .help("Save project (\(shortcuts.display(.save)))")
        }
        ToolbarItemGroup(placement: .primaryAction) {
            Button { model.undo() } label: { Image(systemName: "arrow.uturn.backward") }
                .help("Undo (\(shortcuts.display(.undo)))")
                .disabled(!model.canUndo)
            Button { model.redo() } label: { Image(systemName: "arrow.uturn.forward") }
                .help("Redo (\(shortcuts.display(.redo)))")
                .disabled(!model.canRedo)
            Button { model.clear() } label: { Image(systemName: "trash") }
                .help("Clear the active layer")
            if model.recordingMs > 0 {
                Button { showReplay.toggle() } label: { Image(systemName: "film") }
                    .help("Replay and timelapse")
                    .popover(isPresented: $showReplay, arrowEdge: .bottom) {
                        ReplayBar(model: model)
                            .padding(10)
                    }
            }
            Button { model.exportPNG() } label: { Image(systemName: "square.and.arrow.up") }
                .help("Export PNG (\(shortcuts.display(.export)))")
            if historyBranches {
                Button { showBranches.toggle() } label: {
                    Image(systemName: "arrow.triangle.branch")
                }
                .help("History branches")
                .popover(isPresented: $showBranches, arrowEdge: .bottom) {
                    HistoryBranchPanel(model: model)
                }
            }
        }
    }

    /// Reopen the saved window state unless a recovery copy is waiting: recovery is newer and
    /// takes precedence.
    private func restoreSessionIfPossible() {
        guard !restoredSession, recovery == nil else { return }
        restoredSession = true
        guard let session = Session.load(),
              let path = session.document
        else { return }
        if model.openDocument(URL(fileURLWithPath: path)) {
            session.apply(to: model, workspace: workspace)
            showWelcome = false
        }
    }

}
