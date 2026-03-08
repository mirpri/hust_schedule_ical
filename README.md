# HUST 课表 ICS 生成器

HUST（华中科技大学）课表获取与 iCalendar (`.ics`) 文件生成工具。可以将你在 Hub 系统的教务课表导出为标准日历格式，方便倒入到 Outlook、Google Calendar、Apple Calendar 以及各类手机系统日历中。

by Mirpri

## 功能特点

- **自动获取**：自动唤起本地的 Chrome 或 Edge 浏览器打开 Hub 登录页面，登录完成后自动抓取并提取所需要的身份凭证。
- **自定义作息时间**：支持通过 json 配置文件，自定义上课时间规则。
- **夏季秋季作息自动切换**：夏令时/冬令时根据日期自动切换。
- **配置保存**：自动记录使用习惯，保存在 `settings.json` 中，下次使用无需重复输入。
- **多平台支持**：支持 Windows, macOS 和 Linux。

## 下载与运行

你可以从仓库的 [Release](https://github.com/mirpri/hust_schedule_ical/releases) 页面下载**适合你操作系统**的压缩文件。
架构（aarch/x86）与操作系统（Windows/macOS/Linux）都要与电脑匹配。

压缩文件包含了可执行文件和必须的 `.json` 配置文件。

### Windows 用户建议
直接双击运行 `hust_schedule_ical.exe` 即可。

## 使用步骤

1. 运行本程序。
2. 程序会自动打开一个空白的浏览器窗口并跳转到 HUST 的 Hub 统一身份认证系统。
3. 在弹出的浏览器窗口中完成账号登录。
4. **登录成功后，回到原本的黑色命令行/终端窗口，按下 `Enter`（回车键）继续**。
5. 程序会自动抓取课表、分析处理，并在同级目录下生成 `schedule.ics` 文件。
6. 将生成的 `.ics` 文件发送到手机或电脑上，使用任意日历应用打开并导入即可。

## 命令行参数 (CLI)

除了直接双击运行，本程序也支持详细的命令行参数。可以通过以下命令查看所有可用选项：

```bash
hust_schedule_ical --help
```

常见的选项（基于代码推断）：
- 自定义课表时间文件路径
- 使用本地已有的 JSON 课表原始数据代替网络请求获取
- 自定义输出的 `.ics` 文件名路径

## 从源码构建

确保你的电脑上已经安装了 [Rust](https://www.rust-lang.org/zh-CN/tools/install) 工具链。

```bash
# 克隆仓库
git clone https://github.com/mirpri/hust_schedule_ical.git
cd hust_schedule_ical

# 编译执行
cargo run --release
```

## 注意事项

- 程序需要你本机安装有 Chrome 或 Edge 浏览器（提取 Cookie 必须依赖浏览器的调试接口）。
- 如果导入日历后发现上课时间有偏移，请检查当前所用的是否为正确的夏/冬令时作息时间文件（如 `class-times-summer.json` 或 `class-times-fall.json`）并在配置中指定。
