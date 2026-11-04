package control

import (
	"errors"
	"github.com/jackc/pgx/v5/pgxpool"
	"net/url"
	"os"
)

// This explicit local profile never consults PGSERVICE, .pgpass, or inherited PG* settings.
// TLS/SASL production profiles require separate validation and are not silently downgraded.
func explicitConfig(dsn string) (*pgxpool.Config, error) {
	u, e := url.Parse(dsn)
	if e != nil || (u.Scheme != "postgres" && u.Scheme != "postgresql") || u.User == nil || u.User.Username() == "" || u.Hostname() == "" || u.Port() == "" || u.Path == "" || u.Path == "/" {
		return nil, errors.New("CONTROL_EXPLICIT_DEVELOPMENT_URI_REQUIRED")
	}
	for _, key := range postgresEnvironmentKeys {
		if os.Getenv(key) != "" {
			return nil, errors.New("CONTROL_ENVIRONMENT_OVERRIDES_FORBIDDEN")
		}
	}
	q := u.Query()
	for key := range q {
		if key != "sslmode" && key != "password" {
			return nil, errors.New("CONTROL_DSN_UNSUPPORTED_PARAMETER")
		}
	}
	if q.Get("sslmode") != "disable" {
		return nil, errors.New("CONTROL_TLS_PROFILE_NOT_IMPLEMENTED")
	}
	user := u.User.Username()
	password, suppliedPassword := u.User.Password()
	if !suppliedPassword {
		password = q.Get("password")
	}
	u.User = url.UserPassword(user, password)
	q.Del("service")
	q.Del("servicefile")
	q.Set("passfile", os.DevNull)
	q.Set("application_name", "xone-chain-parser-go")
	q.Set("target_session_attrs", "any")
	q.Set("connect_timeout", "3")
	u.RawQuery = q.Encode()
	cfg, e := pgxpool.ParseConfig(u.String())
	if e != nil {
		return nil, errors.New("CONTROL_DSN_INVALID")
	}
	cfg.ConnConfig.Password = password
	cfg.ConnConfig.RuntimeParams = map[string]string{"application_name": "xone-chain-parser-go"}
	cfg.ConnConfig.Fallbacks = nil
	if cfg.ConnConfig.TLSConfig != nil {
		return nil, errors.New("CONTROL_TLS_PROFILE_NOT_IMPLEMENTED")
	}
	return cfg, nil
}

var postgresEnvironmentKeys = []string{"PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD", "PGPASSFILE", "PGAPPNAME", "PGCONNECT_TIMEOUT", "PGSSLMODE", "PGSSLKEY", "PGSSLCERT", "PGSSLSNI", "PGSSLROOTCERT", "PGSSLPASSWORD", "PGSSLNEGOTIATION", "PGTARGETSESSIONATTRS", "PGSERVICE", "PGSERVICEFILE", "PGTZ", "PGOPTIONS", "PGMINPROTOCOLVERSION", "PGMAXPROTOCOLVERSION", "PGCHANNELBINDING", "PGREQUIREAUTH"}
