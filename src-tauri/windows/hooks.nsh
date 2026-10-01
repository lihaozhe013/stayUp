!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\stayup.exe" --stayup-uninstall-managed' $0
  ${If} $0 <> 0
    MessageBox MB_ICONSTOP "StayUp could not remove its managed apps. Resolve the listed service errors and try uninstalling again."
    Abort
  ${EndIf}
!macroend
