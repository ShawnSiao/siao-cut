!macro NSIS_HOOK_PREUNINSTALL
  IfSilent siaocut_keep_ai_services 0
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时删除 SiaoCut 的 AI 服务配置和已保存的 API Key？$\r$\n$\r$\n选择“否”会保留配置，便于重新安装后继续使用；项目、媒体和本地资源不会被删除。" IDNO siaocut_keep_ai_services
  ExecWait '"$INSTDIR\siaocut-core.exe" --json ai-services purge --confirm'
siaocut_keep_ai_services:
!macroend
