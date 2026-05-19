import Foundation
import Orch8Mobile

class EchoHandler: StepHandler {
    func execute(stepName: String, input: String) -> String {
        return "{\"ok\":true}"
    }
}

class AppListener: EngineListener {
    weak var viewModel: SequenceViewModel?

    func onInstanceCompleted(instanceId: String, output: String) {
        DispatchQueue.main.async { [weak self] in
            self?.viewModel?.log("Completed: \(instanceId.prefix(8))...")
        }
    }

    func onInstanceFailed(instanceId: String, error: String) {
        DispatchQueue.main.async { [weak self] in
            self?.viewModel?.log("Failed: \(instanceId.prefix(8))... \(error)")
        }
    }

    func onStepPending(instanceId: String, stepName: String, handler: String) {
        DispatchQueue.main.async { [weak self] in
            self?.viewModel?.log("Waiting: \(stepName) on \(instanceId.prefix(8))...")
        }
    }
}

@MainActor
class SequenceViewModel: ObservableObject {
    @Published var isInitialized = false
    @Published var isRunning = false
    @Published var logEntries: [String] = []
    @Published var instances: [(String, String, String)] = [] // (id, sequence, state)
    @Published var currentTenant: String = "tenant-alpha"

    private var engine: MobileEngine?
    private let listener = AppListener()

    private let serverUrl = "http://localhost:8080"
    private let tenants = ["tenant-alpha", "tenant-beta"]

    func initializeSDK() {
        log("Initializing for \(currentTenant)...")
        listener.viewModel = self

        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("orch8-\(currentTenant)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dbPath = dir.appendingPathComponent("engine.db").path

        let deviceId = "\(currentTenant)-device-\(UUID().uuidString.prefix(8))"

        let config = MobileEngineConfig(
            tickIntervalMs: 100,
            maxConcurrentSteps: 4,
            maxStepsPerInstance: 1000,
            maxConcurrentInstances: 10,
            maxTickDurationMs: 5000,
            maxInstanceLifetimeSecs: 86400,
            maxStoredSequences: 50,
            maxSequenceSizeBytes: 1_048_576,
            handlerTimeoutMs: 30000,
            operationTimeoutMs: 10_000,
            telemetryEnabled: false,
            environment: "test",
            rootPublicKey: "",
            sdkVersion: "0.4.0",
            memoryBudgetBytes: 0,
            sequencesUrl: "",
            syncUrl: "\(serverUrl)/mobile/sync",
            deviceId: deviceId,
            syncApiKey: ""
        )

        do {
            engine = try MobileEngine(dbPath: dbPath, config: config)

            let handlers = [
                "init_payment", "validate_amount", "fraud_check",
                "risk_assessment", "compliance_check", "request_approval",
                "process_payment", "send_receipt", "show_banner",
                "init_profile", "validate_email", "show_terms",
                "collect_preferences", "setup_notifications",
                "complete_onboarding", "check_eligibility",
                "fetch_feature_config", "evaluate_rules",
                "request_consent", "verify_identity", "activate_feature",
            ]
            for name in handlers {
                try engine?.registerHandler(name: name, handler: EchoHandler())
            }

            engine?.setListener(listener: listener)

            // Register device with server
            registerDevice(deviceId: deviceId)

            isInitialized = true
            log("SDK initialized (device: \(deviceId.prefix(16))...)")
        } catch {
            log("Init failed: \(error.localizedDescription)")
        }
    }

    func loadWorkflow(_ name: String) {
        guard let engine else { return }
        log("Loading \(name)...")

        let url = Bundle.main.url(forResource: name, withExtension: "json")
            ?? URL(fileURLWithPath: "\(Bundle.main.bundlePath)/\(name).json")

        // Try loading from bundle first, then from hard-coded JSON
        if let url = Bundle.main.url(forResource: name, withExtension: "json"),
           let data = try? Data(contentsOf: url),
           let json = String(data: data, encoding: .utf8) {
            do {
                try engine.loadSequenceFromJson(json: json)
                log("Loaded \(name)")
            } catch {
                log("Load failed: \(error.localizedDescription)")
            }
            return
        }

        // Fallback: fetch from server
        log("Fetching \(name) from server...")
        Task {
            do {
                let count = try engine.loadSequencesFromUrl(url: "\(serverUrl)/sequences?namespace=default")
                log("Loaded \(count) sequences from server")
            } catch {
                log("Fetch failed: \(error.localizedDescription)")
                // Load embedded payment-verification workflow
                loadEmbeddedWorkflow()
            }
        }
    }

    func startWorkflow(_ name: String) {
        guard let engine else { return }
        log("Starting \(name)...")
        do {
            let id = try engine.start(sequenceName: name, input: "{}", dedupKey: nil)
            log("Started: \(id.prefix(8))...")
            refreshInstances()
        } catch {
            log("Start failed: \(error.localizedDescription)")
        }
    }

    func resume() {
        engine?.resume()
        isRunning = true
        log("Engine resumed")
    }

    func pause() {
        engine?.pause()
        isRunning = false
        log("Engine paused")
    }

    func refreshInstances() {
        guard let engine else { return }
        do {
            let active = try engine.activeInstances()
            instances = active.map { ($0.instanceId, $0.sequenceName, "\($0.state)") }
        } catch {
            log("Refresh failed: \(error.localizedDescription)")
        }
    }

    func switchTenant() {
        shutdown()
        currentTenant = currentTenant == "tenant-alpha" ? "tenant-beta" : "tenant-alpha"
        isInitialized = false
        isRunning = false
        instances = []
        log("Switched to \(currentTenant)")
    }

    func shutdown() {
        engine?.shutdown()
        engine = nil
        isRunning = false
        log("Engine shutdown")
    }

    func log(_ message: String) {
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        logEntries.append("[\(fmt.string(from: Date()))] \(message)")
        if logEntries.count > 100 { logEntries.removeFirst() }
    }

    private func registerDevice(deviceId: String) {
        guard let url = URL(string: "\(serverUrl)/mobile/devices/register") else { return }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(currentTenant, forHTTPHeaderField: "X-Tenant-Id")
        request.httpBody = try? JSONSerialization.data(withJSONObject: [
            "device_id": deviceId,
            "platform": "ios",
            "app_version": "0.4.0"
        ])
        URLSession.shared.dataTask(with: request) { [weak self] _, resp, err in
            DispatchQueue.main.async {
                if let err { self?.log("Register failed: \(err.localizedDescription)") }
                else { self?.log("Device registered") }
            }
        }.resume()
    }

    private func loadEmbeddedWorkflow() {
        guard let engine else { return }
        let json = """
        {
          "id": "\(UUID().uuidString)",
          "tenant_id": "mobile",
          "namespace": "default",
          "name": "payment-verification",
          "version": 1,
          "deprecated": false,
          "blocks": [
            {"type":"step","id":"init_payment","handler":"init_payment","params":{},"cancellable":true},
            {"type":"step","id":"validate_amount","handler":"validate_amount","params":{},"cancellable":true},
            {"type":"step","id":"fraud_check","handler":"fraud_check","params":{},"cancellable":true},
            {
              "type":"step","id":"payment_approval","handler":"request_approval",
              "params":{},"cancellable":true,
              "wait_for_input":{
                "prompt":"Payment requires authorization. Approve or reject.",
                "timeout":86400000,
                "choices":[
                  {"label":"Approve","value":"approved"},
                  {"label":"Reject","value":"rejected"}
                ],
                "store_as":"payment_decision"
              }
            },
            {"type":"step","id":"process_payment","handler":"process_payment","params":{},"cancellable":true},
            {"type":"step","id":"send_receipt","handler":"send_receipt","params":{},"cancellable":true}
          ],
          "created_at": "2026-05-19T00:00:00Z"
        }
        """
        do {
            try engine.loadSequenceFromJson(json: json)
            log("Loaded embedded payment-verification")
        } catch {
            log("Embedded load failed: \(error.localizedDescription)")
        }
    }
}
