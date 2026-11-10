# 设置补正启动器

把 Fork 构建产物 `fefw-mod-launcher.exe` 放进已完成游戏准备的普通版目录，与官方 `fefw-launcher.exe` 并列。使用 `fefw-mod-launcher.exe` 启动。

- 存在 `mods/设置补正` 时，启动器在每次点击开始游戏时读取本机构建清单，并检查两版的游戏 Build ID 和运行库 SHA-256。
- 把 `设置补正` 文件夹移出 `mods` 后，再点击开始游戏会启动普通版。
- MOD 的设置和存档放在单独的运行目录。这个目录与 `mods` 同级，移动 MOD 文件夹时可以保留原有进度。
- 校验失败时会显示错误。可以把 MOD 文件夹移出 `mods`，或重新适配并构建本机 MOD。

## 安装本机已有的 MOD

在 Windows 上运行以下命令。目标目录须先有一套已准备好的普通版游戏，以及一个已有独立 `fefw.ini` 和 `save` 的 MOD 运行目录。

```powershell
python launcher/scripts/stage_settings_mod.py --game 'D:\fefw-recomp' --build 'D:\local-mod-build' --profile 'mod-profile'
```

`--build` 指向本机已有的 MOD 构建，其中包括 `fefw.exe`、`fefw_rt.dll` 和六个 `exefs` 模块。`--profile` 只填运行目录的名称。脚本先在同盘临时目录准备文件，全部写完再移动到 `mods/设置补正`。已有的 MOD 目录需要先移走。

## 上游更新

在 Fork 的 Actions 启用 **Build settings launcher**。默认分支每 12 小时检查 Goldoire/fefw-recomp 最新正式版，也可以手动运行。发现新标签时在临时分支合并上游，运行 Rust 测试并构建 Windows 启动器。成功后工作流更新 Fork 的默认分支，构建产物可从该次 Actions 运行下载。合并冲突、构建失败或默认分支在构建期间变动都会阻止自动推进。

GitHub 构建的是启动器。游戏构建和 MOD 适配使用玩家本机的文件。上游修改游戏 Build ID 或运行库时，需要在本机重新验证 MOD，再安装新的构建。请把游戏转储、游戏二进制、密钥、存档和本机 MOD 构建留在 Git 工作区以外。
