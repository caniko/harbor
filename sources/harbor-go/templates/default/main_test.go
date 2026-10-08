package main

import "testing"

func TestEmbeddedGreeting(t *testing.T) {
	t.Parallel()
	if got := message(); got != "dev: hello from harbor-go" {
		t.Fatalf("message() = %q, want embedded greeting", got)
	}
}
