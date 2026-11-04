package control

import (
	"strings"
	"testing"
)

func TestPostgresEnvironmentCannotOverrideFileURI(t *testing.T) {
	for _, key := range postgresEnvironmentKeys {
		t.Setenv(key, "")
	}
	cfg, e := explicitConfig("postgresql://owner@127.0.0.1:54321/xone?sslmode=disable")
	if e != nil {
		t.Fatal(e)
	}
	if cfg.ConnConfig.Host != "127.0.0.1" || cfg.ConnConfig.Port != 54321 || cfg.ConnConfig.User != "owner" || cfg.ConnConfig.Database != "xone" || cfg.ConnConfig.Password != "" {
		t.Fatal("wrong file config")
	}
	for _, key := range []string{"PGHOST", "PGPASSWORD", "PGSERVICE", "PGPASSFILE", "PGOPTIONS"} {
		t.Setenv(key, "do-not-inherit-secret")
		_, e = explicitConfig("postgresql://owner@127.0.0.1:54321/xone?sslmode=disable")
		if e == nil || strings.Contains(e.Error(), "do-not-inherit-secret") {
			t.Fatal("environment accepted or leaked")
		}
		t.Setenv(key, "")
	}
}
