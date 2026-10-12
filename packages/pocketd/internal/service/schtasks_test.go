package service

import (
	"encoding/xml"
	"strings"
	"testing"
)

func TestTaskKeepsPocketdRunningForTheUserFromLogon(t *testing.T) {
	got := taskXML(Spec{Exe: `C:\Users\A & B\pocketd.exe`, Home: `C:\Users\A & B\.anywhere\`, Label: "dev.anywhere.pocketd"}, `PC\A & B`)
	for _, want := range []string{
		`<Command>C:\Users\A &amp; B\pocketd.exe</Command>`,
		`<Arguments>serve --home &#34;C:\Users\A &amp; B\.anywhere&#34;</Arguments>`,
		`<UserId>PC\A &amp; B</UserId>`,
		`<LogonTrigger>`,
		`<LogonType>InteractiveToken</LogonType>`,
		`<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>`,
		`<RestartOnFailure>`,
		`<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>`,
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %s in\n%s", want, got)
		}
	}
	d := xml.NewDecoder(strings.NewReader(strings.Replace(got, `encoding="UTF-16"`, `encoding="UTF-8"`, 1)))
	for {
		if _, err := d.Token(); err != nil {
			if err.Error() != "EOF" {
				t.Fatalf("not well-formed: %v", err)
			}
			break
		}
	}
}
