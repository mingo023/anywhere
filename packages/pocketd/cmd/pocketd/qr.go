package main

import (
	"strings"

	"rsc.io/qr"
)

const quietZone = 4

// qrText draws text as a QR code, two modules per character cell, black on
// white so it scans in a dark terminal too.
func qrText(text string) (string, error) {
	code, err := qr.Encode(text, qr.M)
	if err != nil {
		return "", err
	}
	dark := func(x, y int) bool { return code.Black(x-quietZone, y-quietZone) }
	size := code.Size + 2*quietZone
	var b strings.Builder
	for y := 0; y < size; y += 2 {
		b.WriteString("\x1b[30;107m")
		for x := range size {
			switch top, bottom := dark(x, y), dark(x, y+1); {
			case top && bottom:
				b.WriteString("█")
			case top:
				b.WriteString("▀")
			case bottom:
				b.WriteString("▄")
			default:
				b.WriteString(" ")
			}
		}
		b.WriteString("\x1b[0m\n")
	}
	return b.String(), nil
}
