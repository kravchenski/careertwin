@echo off
powershell -NoProfile -ExecutionPolicy Bypass -Command "$w = New-Object -ComObject WScript.Shell; $s = $w.CreateShortcut([Environment]::GetFolderPath('Desktop') + '\CareerTwin Interview.lnk'); $s.TargetPath = '%~dp0python\pythonw.exe'; $s.Arguments = '\"%~dp0start.pyw\"'; $s.WorkingDirectory = '%~dp0'; $s.IconLocation = '%~dp0careertwin-interview.ico'; $s.Save()"
echo A shortcut "CareerTwin Interview" is on your desktop.
pause
