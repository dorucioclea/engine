import SwiftUI

struct ContentView: View {
    @ObservedObject var viewModel: SequenceViewModel

    var body: some View {
        NavigationView {
            VStack(spacing: 16) {
                tenantSection
                controlSection
                instancesSection
                logSection
            }
            .padding()
            .navigationTitle("Orch8 Mobile Sync")
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private var tenantSection: some View {
        GroupBox("Tenant") {
            HStack {
                Text(viewModel.currentTenant)
                    .font(.system(.body, design: .monospaced))
                    .foregroundColor(.primary)
                Spacer()
                Button("Switch") {
                    viewModel.switchTenant()
                }
                .buttonStyle(.bordered)
                .tint(.orange)
            }
        }
    }

    private var controlSection: some View {
        VStack(spacing: 10) {
            HStack(spacing: 12) {
                Button("Init SDK") {
                    viewModel.initializeSDK()
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isInitialized)

                Button(viewModel.isRunning ? "Pause" : "Resume") {
                    if viewModel.isRunning { viewModel.pause() }
                    else { viewModel.resume() }
                }
                .buttonStyle(.bordered)
                .disabled(!viewModel.isInitialized)
            }

            HStack(spacing: 12) {
                Button("Load Workflow") {
                    viewModel.loadWorkflow("payment-verification")
                }
                .buttonStyle(.bordered)
                .disabled(!viewModel.isInitialized)

                Button("Start Workflow") {
                    viewModel.startWorkflow("payment-verification")
                }
                .buttonStyle(.bordered)
                .tint(.green)
                .disabled(!viewModel.isInitialized)
            }
        }
    }

    private var instancesSection: some View {
        GroupBox("Instances (\(viewModel.instances.count))") {
            if viewModel.instances.isEmpty {
                Text("No active instances")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(viewModel.instances, id: \.0) { inst in
                        HStack {
                            Text(String(inst.0.prefix(8)) + "...")
                                .font(.system(.caption, design: .monospaced))
                            Text(inst.1)
                                .font(.caption)
                                .foregroundColor(.secondary)
                            Spacer()
                            Text(inst.2)
                                .font(.caption2)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(stateColor(inst.2).opacity(0.15))
                                .foregroundColor(stateColor(inst.2))
                                .cornerRadius(4)
                        }
                    }
                }
            }
            HStack {
                Spacer()
                Button("Refresh") {
                    viewModel.refreshInstances()
                }
                .font(.caption)
            }
        }
    }

    private var logSection: some View {
        GroupBox("Log") {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 3) {
                        ForEach(Array(viewModel.logEntries.enumerated()), id: \.offset) { idx, entry in
                            Text(entry)
                                .font(.system(.caption2, design: .monospaced))
                                .id(idx)
                        }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: 180, alignment: .topLeading)
                .onChange(of: viewModel.logEntries.count) { _ in
                    if let last = viewModel.logEntries.indices.last {
                        proxy.scrollTo(last)
                    }
                }
            }
        }
    }

    private func stateColor(_ state: String) -> Color {
        switch state.lowercased() {
        case "running": return .blue
        case "waiting": return .orange
        case "completed": return .green
        case "failed": return .red
        case "cancelled": return .gray
        default: return .secondary
        }
    }
}
