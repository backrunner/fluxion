# 浏览器下载接管

Fluxion 提供 Manifest V3 扩展，面向 macOS 的 Chrome、Edge、Brave 和 Chromium（Chromium 132+）。
扩展通过 Native Messaging 把 HTTP/HTTPS GET 下载交给正在运行的 Fluxion；
安装版应用未启动时，本地连接程序会尝试启动它。

## 安装

1. 构建并打开包含浏览器连接程序的新版 Fluxion：

   ```sh
   python3 scripts/bundle-macos.py --release
   open target/native/release/bundle/Fluxion.app
   ```

   正常启动时，Fluxion 自动注册本机 Native Messaging 连接程序。
   将应用移动到 `/Applications` 后再打开一次，可更新连接路径。

2. 打开 `chrome://extensions`（Edge 使用 `edge://extensions`），开启开发者模式，
   选择「加载已解压的扩展程序」，加载仓库的 `browser-extension` 目录。
   应用包内 `Contents/Resources/browser-extension` 也包含同一份扩展。
3. 点击工具栏里的 Fluxion，选择「检查连接」。连接成功后，开启「自动交给 Fluxion」，
   设置大小下限并保存。

扩展尚未发布到浏览器商店。不要删除 manifest 的 `key`：它保证扩展 ID 与本地连接程序的
来源白名单一致。若以后上架商店，应统一更新扩展 ID 和宿主白名单。

源码开发可以单独注册调试连接程序（之后需要手动保持 Fluxion 运行）：

```sh
cargo build -p fluxion-app -p fluxion-browser
./target/debug/fluxion-browser-host --install
./target/debug/fluxion-app
```

## 使用规则

- 自动接管默认关闭；默认大小下限是 **10 MiB**。
- 小于下限的文件继续由浏览器下载；等于下限会接管。`0` 表示接管所有已知大小的文件。
- 大小未知默认跳过，可以单独开启。优先使用浏览器提供的总大小，其次使用原响应的
  Content-Length；不额外请求文件来猜测大小。
- 弹窗里可以把最近开始、仍在下载的某个文件单独交给 Fluxion，不受大小下限限制。
- 接管后打开 Fluxion 原有的「添加任务」窗口，预填链接、文件名和凭据。
  保存目录默认与普通新建任务一致，可修改目录、文件名、连接数和限速后再确认。
  扩展提供的文件名只取 basename，不能指定任意保存目录。
- 设置只对新下载生效。暂停的下载不会自动接管，但可以手动接管。

接管时先暂停浏览器下载，通过本机 IPC 传递 URL、Cookie 等请求头，打开 Fluxion
添加任务窗口。**接收 IPC 不创建任务、不启动下载，也不访问 Keychain**；确认前仅在内存中
保留表单数据。点击添加后，使用普通 `Command::Create` 流程，成功后才通知扩展取消浏览器
下载。新任务与手动新建任务一样进入队列，由任务列表的启动操作开始下载。

取消表单、应用忙或连接失败时，浏览器恢复原下载（原本由用户暂停的下载仍保持暂停）。
已有弹窗时不覆盖当前输入。修改下载链接会清除继承的请求头，避免把凭据发给另一个地址。
如果任务已创建但确认消息丢失，浏览器可能继续下载，Fluxion 中的排队任务仍可手动处理。

## 凭据与边界

- 只继承唯一匹配的**最终下载请求**中的 Cookie、Authorization、Referer、Origin、
  User-Agent、Accept 和 Accept-Language。不会把上一个重定向站点的凭据复制到最终站点。
- 网站如果要求额外的 `X-Api-Key` 等请求头，在「请求凭据」里填写**头名称**，
  值仍从原下载请求获取。Host、Range、代理认证和连接控制头不转发。
- Chromium 对部分 `<a download>` 链接隐藏请求头。对此仅在**顶层页面和最终下载地址同源**时，
  按下载 URL、所属 Cookie store 和当前页面的分区键查询 Cookie（包括 HttpOnly），
  补充浏览器提供的 Referer 和 User-Agent。不读取整个 Cookie 库，也不跨站猜测票据。
  如果配置了额外鉴权头或存在认证挑战，就保留浏览器下载。
  浏览器未暴露的 Authorization、客户端证书、绑定连接的认证无法复制。
- 原请求在扩展内存中最多保留 60 秒、256 条；扩展重启、匹配有歧义或缓存已过期时，
  下载保留在浏览器。扩展持久化存储只有开关、大小和额外头名称，没有请求 URL 或凭据。
- 完整下载 URL 和所有继承头通过 Native Messaging → Unix socket IPC 直接传入表单，
  Keychain 不参与传递。用户确认创建后，沿用普通任务的凭据存储机制（生产环境为 Keychain）。
  SQLite 仅保留凭据引用与移除查询参数的 URL。任务详情隐藏凭据值；错误不会回显原始 URL。
- 任务使用浏览器已经访问到的最终 URL，并禁止再次重定向，以免下载票据被转发给其他站点。
  若该地址后来再次跳转或票据过期，需要从浏览器重新发起下载。
- POST、blob/data、隐身下载、206 部分响应和无法准确对应到原请求的下载不接管。
  一次性链接、禁止重复请求或不支持 Fluxion HEAD/Range 探测的网站仍可能需要留在浏览器。
- Firefox 和 Safari 尚未适配。

宿主仅接受扩展 `mkfokhcdpnpilifmdeecgkghkpicikaa`，严格验证消息版本、字段和长度。
App 通过用户私有目录 `~/.fluxion/browser` 中的 Unix socket 接收消息，目录权限 0700、
socket 权限 0600。宿主不会打开第二个数据库实例。

## 开发验证

```sh
node --test browser-extension/*.test.mjs
cargo test -p fluxion-browser
cargo test -p fluxion-core -p fluxion-http -p fluxion-storage -p fluxion-app
```

可选的真实 Chromium 验证需要 Playwright：把 `FLUXION_PLAYWRIGHT_MODULE` 设为其
`index.mjs` 的绝对路径，执行 `node --test browser-extension/chromium.test.mjs`。
该测试加载实际扩展并验证请求观察、HttpOnly Cookie、大小策略、浏览器回退和设置界面；
Native Messaging API 使用测试替身，实际宿主进程由 Rust 集成测试覆盖。

Rust 集成测试使用真实 Native Messaging 子进程和 Unix socket，验证凭据原样到达内存、
等待表单决定、取消和连接断开。GPUI 测试验证表单预填、保存选项修改、确认前无创建命令、
普通创建成功后应答、取消不建任务，以及更换 URL 清除凭据。浏览器测试中的表单决定用
Native Messaging 测试替身模拟；实际 macOS 窗口交互仍需单独人工验证。

预览模式和 `FLUXION_DATA_DIR` 测试进程默认不占用正式浏览器连接。
端到端隔离测试可同时设置 `FLUXION_BROWSER_DIR` 指向短的临时目录；宿主与 App 必须使用
同一设置。macOS Unix socket 路径有长度限制。卸载扩展后，可以删除相应浏览器
`~/Library/Application Support/<浏览器>/NativeMessagingHosts/top.backrunner.fluxion.json`。
