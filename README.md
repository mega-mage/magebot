# magebot

基于 Rust 开发的 Telegram 视频自动下载与直传机器人。支持后台守护进程运行，识别 Telegram 收藏夹（Saved Messages）中的视频链接，自动下载并分块上传回收藏夹，附带交互式 TUI 实时监控控制台。

## 🌟 核心特性

- **自动去重清理**：任务成功/失败后自动撤回原消息及提示，将视频/图片与原链接（附带 `[Uploaded]` / `[Failed]` 前缀）合并发送，防止死循环。
- **推特全媒体极速同步**：
  - **纯图推文**：自动拦截推特图片，通过 FxTwitter API 高清直链多线程下载，跳过 yt-dlp 的缓慢冷启动。
  - **视频/NSFW 敏感推文**：支持直接从 FxTwitter 提取原画 MP4 直链高速下载。若遇到私密推文，自动使用本地 Cookie 唤起 yt-dlp 兜底下载，完美规避推特年龄/敏感内容拦截。
- **智能 Cookie 自适应**：支持使用 `magebot set cookie` 交互式设置 Cookie，自适应支持三种常见的 Cookie 格式：
  1. 浏览器网络请求头中的 **Cookie Header 文本**。
  2. 浏览器插件导出的 **Netscape 规范文本**。
  3. 常见插件导出的 **JSON 数组对象**（如 EditThisCookie / Cookie-Editor 导出的格式）。
  *针对 X/Twitter 平台，生成 Netscape 文件时会自动实现 `.twitter.com` 与 `.x.com` 双域名条目的无缝克隆，防止跨域名权限失效。*
- **分层 TUI 监控 (`ratatui`)**：
  - **左侧**：实时任务列表（上下箭头区分 `📥↓` 下载与 `📤↑` 上传，含进度条、速度及 ETA）。
  - **右侧**：精简过滤后的系统日志，仅显示 Saved Messages 内的动作及系统事件。
  - **底部**：交互式控制台，支持快捷命令。
- **持久控制支持**：支持在 Monitor 界面内通过指令直接启停、连接或终止后台守护进程。
- **多目录与多群组智能分流 (Custom Watch Rules)**：支持配置多个本地监控文件夹，并将不同的文件夹分流直传到不同的 Telegram 目标群组或频道。路径支持波浪号 `~` 自动解析（例如将 `~/.magebot/savings` 解析为系统绝对家目录），并支持使用群组/频道 ID（如 `-100xxxx` 或 `-xxxx`）以及公共用户名（如 `@channel`）作为投递目标。

## ⚙️ 系统依赖说明

在部署运行之前，请确保宿主机安装有以下基础组件：
1. **FFmpeg**：用于音视频格式转换与合并（由系统包管理器提供）。
2. **OpenSSL & SQLite3 开发库**：Rust 编译和会话持久化必须（编译依赖）。
3. **Node.js**：作为 `yt-dlp` 的 JavaScript 运行环境，用于自动解密/计算 YouTube 的 `n` 签名挑战。
4. **yt-dlp**：作为非公开或私密链接的强力下载引擎（保存在项目的 `dependency/` 目录下）。

> [!NOTE]
> 对于 Linux 用户，以上所有编译依赖及二进制依赖（包括自动拉取官方最新 `yt-dlp` 的 Linux 平台执行文件，以及 Node.js 的安装）均已包含在下面的**一键部署脚本**中，无需手动下载。

## 🚀 部署指南 (Linux 一键部署)

我们提供了一键式部署与编译脚本 [**`deploy.sh`**](file:///j:/RustProjects/upload_tel_bot/deploy.sh)，支持在 Debian/Ubuntu/CentOS 服务器上完成一键初始化：

```bash
# 1. 克隆代码仓库并进入目录
git clone https://github.com/mega-mage/magebot.git
cd magebot

# 2. 赋予脚本执行权限并运行
chmod +x deploy.sh
./deploy.sh
```

> 该脚本会自动安装 `ffmpeg`、`curl` 以及编译依赖，配置 `Rust` 工具链，拉取最新 Linux x86_64 版 `yt-dlp` 并放置于 `dependency/`，完成 Release 模式构建，最终将 `magebot` 全局注册到 `/usr/local/bin`。

---

## 🛠️ 快速开始

### 1. 配置参数

配置文件位于 `~/.magebot/config.toml`（在一键脚本运行完后可通过以下命令进行交互式设置）：
```bash
# 依次设置 Telegram API 参数及默认监控文件夹 (默认上传至收藏夹)
magebot set api_id <your_api_id>
magebot set api_hash <your_api_hash>
magebot set phone_number <+86...>
magebot set watch_dir </path/to/watch_folder>

# (可选) 设置多目录直传群组/频道的分流规则 (路径支持 ~ 波浪号自动解析)
magebot set watch_rule "<本地目录路径>:<目标群组ID或用户名>"
# 示例 1: 监控 ~/.magebot/savings 并分传到测试群组 -5589877937
magebot set watch_rule "~/.magebot/savings:-5589877937"
# 示例 2: 监控 D:\Uploads 并分传到公共频道 @my_channel
magebot set watch_rule "D:\Uploads:@my_channel"

# (可选) 删除已有的多目录分流规则
magebot set del_watch_rule "<本地目录路径>"

# (可选) 设置加密存储的平台 Cookie (支持 YouTube / Bilibili / Twitter)
magebot set cookie
```

### 2. 命令行控制 (CLI)

```bash
# 登录授权 Telegram 账号 (首次运行需输入验证码)
magebot login

# 后台守护进程控制
magebot start     # 启动
magebot stop      # 停止
magebot restart   # 重启

# 打开 TUI 实时监控面板
magebot monitor
```

### 3. TUI 控制台命令 (magebot >)

在 `monitor` 底部控制台中输入（支持大小写，可省略 `/` 符号）：
- `download <URL>` : 手动提交视频链接同步任务。
- `start`          : 在后台启动守护进程并自动连接。
- `stop`           : 安全停止后台守护进程。
- `help`           : 列出所有可用命令说明。
- `exit`           : 退出监控面板（保持守护进程运行）。
