# Regras do WhatsApp Dumbphone Gateway

A reescrita nativa em Rust foi autorizada pelo proprietário. Este arquivo é a
fonte de instruções versionada para novas implementações. Refatorações fora do
escopo de uma tarefa exigem autorização; correções necessárias à tarefa fazem
parte do escopo.

## Arquitetura

- Backend nativo em Rust, sem Node, Chromium, SPA ou microsserviços.
- `native/src/domain.rs`: tipos, validação e contrato Gateway.
- `whatsapp.rs`: protocolo, armazenamento e conversão de tipos do fornecedor.
- `http.rs`: transporte HTTP, autenticação de requisições e limites de concorrência.
- `auth.rs`: credenciais, assinaturas e expiração; `store.rs`: sessões e deduplicação.
- `views.rs`: HTML escapado, sem rede ou estado global; `config.rs`: validação inicial.
- `main.rs`: composição, exclusão por diretório e encerramento. Sem efeitos externos
  na importação de módulos. Injete Gateway falso nos testes.
- Um dono da conexão por conta/DATA_DIR. Não alegar suporte horizontal para a mesma
  conta. Evitar filas e tarefas sem limite; controlar admissão e tratar sobrecarga.
- Fixar dependência de protocolo em revisão Git e versionar Cargo.lock. Alterações
  de protocolo exigem regressões e validação manual antes de produção.

## Produto e segurança

- Preservar HTML simples para tela de 240 px, teclado e formulários sem JavaScript.
- Compatibilidade com Nokia 2660 Flip 4G só pode ser declarada após teste no aparelho.
- Preservar modo sem cookies; documentar exposição de tokens em URLs e oferecer
  sessão por cookie. HTTPS é fornecido por proxy, não pelo servidor HTTP.
- Proibir segredos padrão, placeholders ativos e autenticação por token vazio.
- Escapar texto/atributos e codificar URLs. Validar identificadores e mensagens.
- Proteger POSTs contra CSRF e vincular assinaturas a contexto, ação e destinatário.
- Não repetir automaticamente envios ambíguos. Persistir reserva antes do efeito
  externo; repetição do formulário deve ser detectada inclusive após reinício.
- Não registrar mensagens, cookies, tokens, senhas ou QR fora do terminal
  administrativo. Proteger DATA_DIR e nunca versionar dados pessoais ou .env.
- Não remover locks para forçar acesso a perfis. Encerrar com prazo máximo.
- Não confundir histórico vazio com falha de sincronização/consulta.

## Testes e entrega

- Executar `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`
  e `cargo test --locked`. Validar Docker quando runtime/container mudar.
- Testes normais não conectam ao WhatsApp, não pareiam conta e não enviam mensagens
  reais. Testes reais exigem autorização específica e conversa de teste.
- Cobrir autenticação, CSRF, escape, validação, revogação, persistência,
  deduplicação, falhas e sobrecarga. Usar testes de comportamento, sem cobertura
  percentual arbitrária nem testes artificiais de documentação.
- Atualizar README/.env.example quando operação mudar. Informar checks executados,
  limitações e riscos; não alegar desempenho sem medição comparável.
- Não fazer merge ou deploy automaticamente. Branch, commit e push estão no fluxo
  autorizado deste projeto; manter o resultado revisável.
