dev:
	docker-compose up -d

dev-down:
	docker-compose down

start-server:
	cargo watch -q -c -w src/ -x run

install:
	cargo add axum
	cargo add tower-http -F 'cors'
	cargo add mongodb -F 'bson-chrono-0_4'
	cargo add futures --features async-await --no-default-features
	cargo add serde -F derive
	cargo add serde_json
	cargo add thiserror
	cargo add chrono -F serde
	cargo add tokio -F full
	cargo add dotenv
	# HotReload
	cargo install cargo-watch 