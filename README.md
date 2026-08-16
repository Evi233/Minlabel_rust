# Minlabel

合作音频标注桌面客户端（egui）。与 [Minlabel_rust_server](https://github.com/Evi233/Minlabel_rust_server) 配合使用。

## 功能

- 本地文件夹标注：打开文件夹，逐条播放 wav、转写并保存标注（json + lab）
- **房间协作**：创建房间（6 位房间码）或凭码加入，成员实时看到彼此的标注与进度
- **按需上传/下载**：开房只注册文件元数据；选中文件时若服务器没有音频，自动向拥有者请求，拥有者按需上传，请求者下载到本地缓存后播放
- 标注锁定：选中文件即声明（`claim`），他人可见"正在标注"；标注完成自动释放

## 使用

1. 启动服务器（`cargo run --release`，默认 `0.0.0.0:8080`）
2. 客户端 `File → Room`：
   - **Create room**：输入服务器地址、端口、用户名，创建房间（若已打开文件夹，其文件元数据会自动注册）
   - **Join room**：输入服务器地址、端口、用户名、房间码加入
3. 左侧列表显示房间文件；点击文件即声明并自动获取音频（下载或请求拥有者上传）
4. 输入文本后点 Replace / Append 转写并保存，标注会同步给房间所有人

## 构建

```bash
cargo build --release
```

GitHub Actions 自动在 Ubuntu / Windows / macOS 上编译（clippy + build + test），产物作为 artifact 上传。
