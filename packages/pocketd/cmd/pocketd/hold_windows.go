package main

import "errors"

func hold() error { return errors.New("pocketd hold: Windows keeps Terminals inside pocketd") }
