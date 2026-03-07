SHELL := /bin/bash

.DEFAULT_GOAL := help

.PHONY: help backend frontend frontend-install be fe

help:
	@printf "Targets:\n"
	@printf "  make backend          Run the Rust backend\n"
	@printf "  make be               Alias for make backend\n"
	@printf "  make frontend         Run the Next.js frontend\n"
	@printf "  make fe               Alias for make frontend\n"
	@printf "  make frontend-install Install frontend dependencies\n"

backend:
	@./backend/scripts/run-backend.sh

be: backend

frontend:
	@cd frontend && npm run dev

fe: frontend

frontend-install:
	@cd frontend && npm install
