import FileProvider
import Foundation

private struct Reply: Encodable {
    let ok: Bool
    let code: String
}

private func finish(_ reply: Reply, status: Int32) -> Never {
    let data = try! JSONEncoder().encode(reply)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([0x0a]))
    exit(status)
}

@main
private struct DiskSageFileProviderEviction {
    static func main() async {
        guard CommandLine.arguments.count == 3, CommandLine.arguments[1] == "--evict" else {
            finish(Reply(ok: false, code: "invalid-invocation"), status: 64)
        }
        let path = CommandLine.arguments[2]
        guard path.hasPrefix("/"), !path.contains("\0") else {
            finish(Reply(ok: false, code: "invalid-path"), status: 64)
        }

        do {
            let (item, domainIdentifier) = try await NSFileProviderManager.identifierForUserVisibleFile(at: URL(fileURLWithPath: path))
            let domains = try await NSFileProviderManager.domains()
            guard let domain = domains.first(where: { $0.identifier == domainIdentifier }) else {
                finish(Reply(ok: false, code: "domain-not-found"), status: 69)
            }
            guard let manager = NSFileProviderManager(for: domain) else {
                finish(Reply(ok: false, code: "manager-unavailable"), status: 69)
            }
            try await manager.evictItem(identifier: item)
            finish(Reply(ok: true, code: "eviction-request-completed"), status: 0)
        } catch let error as NSError {
            let code = "request-failed:\(error.domain):\(error.code)"
                .unicodeScalars
                .map { CharacterSet.alphanumerics.contains($0) || "-:._".unicodeScalars.contains($0) ? Character(String($0)) : "_" }
            finish(Reply(ok: false, code: String(code.prefix(160))), status: 70)
        }
    }
}
