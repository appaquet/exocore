import SwiftUI
import Exocore

struct DiscoveryView: View {
    @EnvironmentObject var appState: AppState
    @ObservedObject var state: DiscoveryState

    init() {
        self.state = DiscoveryState()
    }

    var body: some View {
        NavigationView {
            VStack {
                discoView()
                Spacer()
                HStack {
                    errorView()
                    Spacer()
                    Button("Save") {
                        self.appState.refreshNodeConfig()
                    }
                    .padding()
                }
            }
            .navigationBarTitle("Bootstrap")
            .onAppear {
                state.performJoinCell(appState)
            }
        }
    }

    func discoView() -> some View {
        if let pin = state.pin {
            return Text("Discovery PIN: \(pin)")
        } else {
            return Text("Joining...")
        }
    }

    func errorView() -> Text? {
        if let err = self.state.error {
            return Text("Error: \(err)").foregroundColor(.red)
        }

        return appState.currentError.map {
            Text("Error: \($0)").foregroundColor(.red)
        }
    }
}


class DiscoveryState: ObservableObject {
    private var discovery: Discovery?
    @Published var pin: UInt32?
    @Published var error: String?

    func performJoinCell(_ appState: AppState) {
        if self.discovery != nil {
            return
        }

        guard let node = appState.node else {
            self.error = "No node configured"
            return
        }

        do {
            let discovery = try Discovery()
            self.discovery = discovery

            discovery.joinCell(localNode: node) { (stage) in
                DispatchQueue.main.async {
                    switch stage {
                    case .pin(let pin):
                        self.pin = pin
                    case .success(let newNode):
                        appState.node = newNode
                        appState.forceDiscovery = false
                        appState.refreshNodeConfig()
                    case .error(let err):
                        self.error = err.localizedDescription
                    }
                }
            }
        } catch {
            self.error = error.localizedDescription
        }

    }
}

#if DEBUG
struct BootstrapView_Previews: PreviewProvider {
    static var previews: some View {
        DiscoveryView()
    }
}
#endif
