package main

import (
	"log"

	"pocketd/internal/registry"
)

// reloadOnRemove has each store drop, once a project is removed, what the database dropped with it.
func reloadOnRemove(reg *registry.Registry, stores ...interface{ Reload() error }) {
	reg.OnRemove(func(string) {
		for _, s := range stores {
			if err := s.Reload(); err != nil {
				log.Printf("project removed: %v", err)
			}
		}
	})
}
