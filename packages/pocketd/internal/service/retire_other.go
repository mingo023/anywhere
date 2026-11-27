//go:build !darwin

package service

func Retire(label, home string) (bool, error) { return false, nil }
