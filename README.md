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

## 🛠️ 快速开始

### 1. 配置参数

配置文件位于 `~/.magebot/config.toml`：
```toml
api_id = 123456
api_hash = "your_api_hash"
download_dir = "downloads"
# 可选设置
# yt_dlp_args = "--format mp4"
```

### 2. 命令行工具 (CLI)

```bash
# 登录授权 Telegram 账号
magebot login

# 交互式设置平台 Cookie (支持 YouTube / Bilibili / Twitter 等)
magebot set cookie

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
