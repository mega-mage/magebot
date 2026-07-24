# magebot

基于 Rust 开发的 Telegram 视频自动下载与直传机器人。支持后台守护进程运行，识别 Telegram 收藏夹（Saved Messages）中的视频链接，自动下载并分块上传回收藏夹，附带全新的**纯鼠标驱动 TUI 图形监控控制台**。

---

## 🌟 核心特性

- **纯鼠标驱动 Tab 式 TUI 监控控制台 (`ratatui` + `crossterm`)**：
  - **📊 概览 Tab**：实时展示守护进程 PID、连接状态、监控规则数及活动传输任务进度条（含下载/上传速度与 ETA）。
  - **📋 规则 Tab**：可视化表格展示所有监控规则，支持点击 `[🔵 ON]` / `[⚫ OFF]` 开关即时开关监听，以及点击 `[➕ 添加规则]` / `[🗑 删除]` 交互式管理。
  - **⚙️ 参数设置 Tab**：编辑并在线更新 `auto_delete``download_dir``yt_dlp_path``yt_dlp_args` 及最大并发上传数，支持一键 `[💾 保存修改]` 到 `config.toml`。
  - **📜 实时日志 Tab**：守护进程日志实时流式查看，支持鼠标滚轮自由上下滑动与暂停/恢复自动滚动。
  - **🔽 视频下载 Tab**：可视化粘贴 URL 发起异步下载任务，内置 `[📋 粘贴]` 按钮关联系统剪贴板。
  - **底部控制栏**：包含 `[▶ 启动服务]``[■ 停止服务]``[↻ 重启服务]` 及 `[🔍 诊断检查]` 快捷按钮，无需打字。
- **自动去重清理**：任务成功/失败后自动撤回原消息及提示，将视频/图片与原链接（附带 `[Uploaded]` / `[Failed]` 前缀）合并发送，防止死循环。
- **推特全媒体极速同步**：
  - **纯图推文**：自动拦截推特图片，通过 FxTwitter API 高清直链多线程下载，跳过 yt-dlp 的缓慢冷启动。
  - **视频/NSFW 敏感推文**：支持直接从 FxTwitter 提取原画 MP4 直链高速下载。若遇到私密推文，自动使用本地 Cookie 唤起 yt-dlp 兜底下载，完美规避推特年龄/敏感内容拦截。
- **智能 Cookie 加密与管理**：支持使用 `magebot set cookie` 交互式设置 Cookie，自适应支持三种常见的 Cookie 格式：
  1. 浏览器网络请求头中的 **Cookie Header 文本**。
  2. 浏览器插件导出的 **Netscape 规范文本**。
  3. 常见插件导出的 **JSON 数组对象**（如 EditThisCookie / Cookie-Editor 导出的格式）。
  *针对 X/Twitter 平台，生成 Netscape 文件时会自动实现 `.twitter.com` 与 `.x.com` 双域名条目的无缝克隆，防止跨域名权限失效。*
- **多目录与多群组智能分流 (Custom Watch Rules)**：支持配置多个本地监控文件夹，并将不同的文件夹分流直传到不同的 Telegram 目标群组或频道。路径支持波浪号 `~` 自动解析，并支持使用群组/频道 ID（如 `-100xxxx`）以及公共用户名（如 `@channel`）作为投递目标。

---

## ⚙️ 系统依赖说明

在部署运行之前，请确保宿主机安装有以下基础组件：
1. **FFmpeg**：用于音视频格式转换与合并（由系统包管理器提供）。
2. **OpenSSL & SQLite3 开发库**：Rust 编译和会话持久化必须（编译依赖）。
3. **Node.js**：作为 `yt-dlp` 的 JavaScript 运行环境，用于自动解密/计算 YouTube 的 `n` 签名挑战。
4. **yt-dlp**：作为非公开或私密链接的强力下载引擎（保存在项目的 `dependency/` 目录下）。

> [!NOTE]
> 对于 Linux 用户，以上所有编译依赖及二进制依赖（包括自动拉取官方最新 `yt-dlp` 的 Linux 平台执行文件，以及 Node.js 的安装）均已包含在下面的**一键部署脚本**中，无需手动下载。

---

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

---

## 🛠️ 快速开始

### 1. 配置与登录

```bash
# 1. 账号登录授权 (首次运行会自动交互式引导输入 Telegram API ID, API Hash 及手机号)
magebot login

# 2. 打开 TUI 图形控制台进行所有配置与服务管理
magebot monitor
```

### 2. 命令行控制 (CLI)

```
magebot - Telegram 视频自动下载与直传工具

账号管理:
  magebot login                           交互式登录 Telegram 账号
  magebot logout                          退出登录并清除会话

配置管理:
  magebot set <参数名> <值>                设置配置参数
  magebot set cookie                      交互式设置平台 Cookie

监控规则:
  magebot add <目录路径>[:<目标群组>]       添加监控规则 (默认投递至收藏夹)
  magebot rm <规则ID或目录路径>             删除监控规则
  magebot ls                              列出所有监控规则
  magebot listen <规则ID> <true|false>     开/关指定规则的媒体链接监听

服务控制:
  magebot start                           启动后台守护进程
  magebot stop                            停止后台守护进程
  magebot restart                         重启后台守护进程
  magebot status                          查看守护进程运行状态
  magebot monitor                         打开纯鼠标 TUI 图形监控面板

诊断:
  magebot check                           检查配置与授权状态
```

---

## 🖥️ 鼠标 TUI 面板交互说明 (`magebot monitor`)

运行 `magebot monitor` 打开图形界面：
- **Tab 标签导航**：使用鼠标直接点击顶部 `[📊 概览]`、`[📋 监控规则]`、`[⚙️ 参数设置]`、`[📜 实时日志]`、`[🔽 视频下载]` 进行切换。
- **数据管理**：在 `[📋 监控规则]` 中点击 `[🔵 ON]` / `[⚫ OFF]` 开关可实时开启/关闭指定规则的聊天链接监听，点击 `[🗑]` 删除规则。
- **视频下载**：在 `[🔽 视频下载]` 中粘贴 URL 或点击 `[📋 粘贴]` 从系统剪贴板获取视频链接，点击 `[⬇ 开始异步下载]` 提交任务。
- **底部服务操作**：直接鼠标点击 `[▶ 启动服务]`、`[■ 停止服务]`、`[↻ 重启服务]` 及 `[🔍 诊断检查]` 一键控制后台守护进程。
- **退出面板**：按 `Esc` 键退出图形控制台（后台守护进程保持正常工作）。

---

## 🗑️ 一键卸载 (Linux)

我们提供了一键卸载脚本 [**`uninstall.sh`**](file:///j:/RustProjects/upload_tel_bot/uninstall.sh) 用于安全停止后台守护进程并清理相关配置：

```bash
chmod +x uninstall.sh
./uninstall.sh
```
