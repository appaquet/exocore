
import UIKit

class ViewController: UIViewController {
    var context: OpaquePointer?
    var queryHandle: ExocoreQueryHandle?

    override func viewDidLoad() {
        super.viewDidLoad()

        let res = exocore_context_new();
        if res.status == UInt8(ExocoreStatus_Success.rawValue) {
            self.context = res.context
        }
    }

    @IBAction func queryClick(_ sender: Any) {
        self.queryHandle = exocore_watched_query(self.context, "hello world", { (status, json) in
            guard let jsonString = json, let nativeJsonString = String(utf8String: jsonString) else {
                print("Got json string that couldn't read")
                return
            }

            print("Got result \(status) \(nativeJsonString)")
        })
    }

    @IBAction func cancelClick(_ sender: Any) {
        if let handle = self.queryHandle {
            exocore_query_cancel(self.context, handle)
        }
    }
}

