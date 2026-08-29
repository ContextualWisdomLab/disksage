import CryptoKit
import FileProvider
import Foundation

private struct Reply: Encodable {
    let ok: Bool
    let code: String
    let identityFingerprint: String?
}

private func finish(_ reply: Reply, status: Int32) -> Never {
    FileHandle.standardOutput.write(try! JSONEncoder().encode(reply))
    FileHandle.standardOutput.write(Data([0x0a]))
    exit(status)
}

private func fingerprint(item: NSFileProviderItemIdentifier, domain: NSFileProviderDomainIdentifier) -> String {
    var input = Data("disksage-file-provider-item-domain-v1\0".utf8)
    input.append(Data(domain.rawValue.utf8))
    input.append(0)
    input.append(Data(item.rawValue.utf8))
    return SHA256.hash(data: input).map { String(format: "%02x", $0) }.joined()
}

@main
private struct DiskSageFileProviderEviction {
    static func main() async {
        let arguments = CommandLine.arguments
        guard (arguments.count == 3 && arguments[1] == "--resolve") ||
              (arguments.count == 4 && arguments[1] == "--evict") else {
            finish(Reply(ok: false, code: "invalid-invocation", identityFingerprint: nil), status: 64)
        }
        let path = arguments[2]
        guard path.hasPrefix("/"), !path.contains("\0") else {
            finish(Reply(ok: false, code: "invalid-path", identityFingerprint: nil), status: 64)
        }

        do {
            let (item, domainIdentifier) = try await NSFileProviderManager.identifierForUserVisibleFile(at: URL(fileURLWithPath: path))
            let identity = fingerprint(item: item, domain: domainIdentifier)
            let domains = try await NSFileProviderManager.domains()
            guard let domain = domains.first(where: { $0.identifier == domainIdentifier }) else {
                finish(Reply(ok: false, code: "domain-not-found", identityFingerprint: identity), status: 69)
            }
            guard let manager = NSFileProviderManager(for: domain) else {
                finish(Reply(ok: false, code: "manager-unavailable", identityFingerprint: identity), status: 69)
            }
            if arguments[1] == "--resolve" {
                finish(Reply(ok: true, code: "identity-resolved", identityFingerprint: identity), status: 0)
            }
            guard arguments[3] == identity else {
                finish(Reply(ok: false, code: "identity-mismatch", identityFingerprint: identity), status: 65)
            }
            try await manager.evictItem(identifier: item)
            finish(Reply(ok: true, code: "eviction-request-completed", identityFingerprint: identity), status: 0)
        } catch let error as NSError {
            let code = "request-failed:\(error.domain):\(error.code)"
                .unicodeScalars
                .map { CharacterSet.alphanumerics.contains($0) || "-:._".unicodeScalars.contains($0) ? Character(String($0)) : "_" }
            finish(Reply(ok: false, code: String(code.prefix(160)), identityFingerprint: nil), status: 70)
        }
    }
}
