package main

import (
	_ "embed"
	"fmt"
	"strings"
)

//go:embed greeting.txt
var greeting string

var version = "dev"

func message() string { return version + ": " + strings.TrimSpace(greeting) }

func main() { fmt.Println(message()) }
