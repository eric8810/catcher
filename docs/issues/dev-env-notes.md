# 开发环境注意事项

- exec 工具（uv_spawn）不加载 PowerShell `$PROFILE`，用户级 PATH 中的 `pnpm` 不可见。需用 `powershell -Command pnpm` 包装执行。
