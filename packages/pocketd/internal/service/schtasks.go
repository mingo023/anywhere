package service

import (
	"bytes"
	"encoding/xml"
	"strings"
)

// taskXML is the scheduled task that keeps pocketd running for user: started
// at their logon and now, in their session, never timed out, restarted a
// minute after it fails. A task can't set env, so serve gets --home.
func taskXML(s Spec, user string) string {
	esc := func(v string) string {
		var b bytes.Buffer
		xml.EscapeText(&b, []byte(v))
		return b.String()
	}
	// A backslash before the closing quote would escape it.
	args := `serve --home "` + strings.TrimRight(s.Home, `\`) + `"`
	return `<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Anywhere host: keeps terminals running for the desktop.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>` + esc(user) + `</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>` + esc(user) + `</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <Enabled>true</Enabled>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>` + esc(s.Exe) + `</Command>
      <Arguments>` + esc(args) + `</Arguments>
      <WorkingDirectory>` + esc(s.Home) + `</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
`
}
