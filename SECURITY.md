# Security Policy / 安全策略

## Supported versions / 支持版本

SiaoCut has no formal Release and no version currently receives production security support. Source builds and unsigned candidates are development artifacts.

SiaoCut 尚无正式 Release，当前没有提供生产安全支持的版本。源码构建和未签名候选包都属于开发制品。

## Report a vulnerability / 报告安全问题

Private vulnerability reporting is not currently enabled for this repository. Do not publish credentials, private keys, media, transcripts, local paths, personal data, or working exploit details in a public issue.

当前仓库尚未启用私密漏洞报告。公开 Issue 中不得提交凭据、私钥、媒体、文稿、本机路径、个人信息或可直接利用的攻击细节。

For a potentially sensitive issue:

1. Open a minimal [GitHub Issue](https://github.com/ShawnSiao/siao-cut/issues) that states only that private security contact is required.
2. Do not include reproduction details or attachments.
3. Wait for the maintainer to provide a private channel.

对于可能涉及敏感信息的问题：

1. 创建最小化的 [GitHub Issue](https://github.com/ShawnSiao/siao-cut/issues)，只说明需要私密安全联系渠道。
2. 不附带复现细节或文件。
3. 等待维护者提供私密渠道。

Non-sensitive hardening requests and dependency notices may use a normal issue after local paths and personal information are removed.

不含敏感细节的安全加固建议和依赖提醒，可以在移除本机路径与个人信息后使用普通 Issue。

## Security boundaries / 安全边界

- Media processing is local. External Agents receive transcript text, timestamps, and structural constraints, never media paths or bytes.
- MOSS accepts only an explicitly configured loopback HTTP service and stores no API key.
- The Rust Core is the only project writer. Media SHA-256 checks protect relinking and export.
- Signing keys, certificates, passwords, and completed private acceptance evidence must remain outside the repository.

- 媒体处理在本机完成。外部 Agent 只接收文稿、时间戳和结构约束，不接收媒体路径或字节。
- MOSS 只接受明确配置的本机回环 HTTP 服务，不保存 API 密钥。
- Rust Core 是项目的唯一写入者；媒体 SHA-256 校验保护重新关联与导出。
- 签名密钥、证书、密码和已填写的私有验收证据必须保存在仓库外。
