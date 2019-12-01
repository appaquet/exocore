
import UIKit

class ViewController: UIViewController {
    var context: OpaquePointer?

    override func viewDidLoad() {
        super.viewDidLoad()

        let res = exocore_context_new();
        if res.status == UInt8(ExocoreStatus_Success.rawValue) {
            self.context = res.context
        }
    }

    @IBAction func buttonClick(_ sender: Any) {
        exocore_watched_query(self.context, "hello world", { (status, json) in
            guard let jsonString = json, let nativeJsonString = String(utf8String: jsonString) else {
                print("Got json string that couldn't read")
                return
            }

            print("Got result \(status) \(nativeJsonString)")
        })
    }
}

