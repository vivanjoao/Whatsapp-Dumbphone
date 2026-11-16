# Correções da revisão de d931439

Os dez achados da revisão foram tratados na branch `rewrite/native-rust`.
A aplicação continua nativa, com formulários sem JavaScript e sem conexão a uma
conta real durante a validação.

| Achado | Correção | Regressão |
| --- | --- | --- |
| P1 — Encerramento preso em I/O | Cliente HTTP explícito com deadlines dentro do worker; runtime com teardown limitado e lock retido até sair | Headers/corpos incompletos, streaming parado/trickle e subprocesso com worker permanentemente bloqueado; sucesso e falha de startup |
| P2 — Assinatura com duas representações | Assinatura HMAC hexadecimal minúscula canônica exigida antes da deduplicação | Mesmo formulário com assinatura em maiúsculas retorna 403 e só dispara um envio |
| P2 — Login multiaba | Reutilizar contexto pré-login válido | Primeiro formulário continua válido após abrir a segunda aba; contexto inválido é renovado |
| P2 — Nome conhecido ausente | Resolver nome do chat, contato e só então JID | ChatStore real com nome ausente, vazio, grupo nomeado e fallback JID |
| P2 — Conversas que retornavam 400 | Paginação keyset filtra tipos não suportados antes de preencher a lista | 66 entradas excluídas antes de 22 suportadas; retorna os 20 chats suportados mais recentes |
| P2 — Não lido manual persistente | Reconhecer marcador −1 ao abrir | Marca −1 dispara leitura; contador zero não dispara outra chamada |
| P2 — Mensagem enviada fora do painel | Fragmento no fim do histórico em envio e refresh; refresh usa nonce público novo para forçar GET | Laptop 1366×768 com histórico transbordando, POST e dois refreshes com nova requisição e última mensagem dentro do retângulo do painel |
| P3 — Senha maior que o campo | Configuração rejeita mais de 1024 unidades UTF-16 | Fronteiras ASCII e emojis, mantendo mínimo de 12 caracteres |
| P3 — Unicode truncado no navegador | Remover maxlength da mensagem; validação de 4096 escalares Unicode no servidor | Colagem normal de 4096 emojis preservada; HTTP aceita 4096 e rejeita 4097 |
| P3 — Texto auxiliar com contraste baixo | Cores mais escuras na base e no CSS responsivo | Contraste mínimo 4,5:1 para texto visível abaixo de 12 px nos viewports exercitados |

O nonce de refresh é público e não contém credencial ou assinatura CSRF. Token e
modo compacto continuam preservados onde exigidos pelo modo de autenticação.
O checker de navegador normaliza URLs codificadas e mede geometria de conteúdo;
`is_visible()` sozinho não prova que uma mensagem está dentro de uma área rolável.

## Validação

- `cargo fmt --all --check`: passou.
- `cargo clippy --locked --all-targets -- -D warnings`: passou.
- `cargo test --locked`: 30 testes passaram (8 biblioteca, 1 runtime, 21 HTTP).
- `scripts/check_layout.py`: passou em 11 combinações de tela/modo, de 128 a 1440 px,
  com JavaScript da aplicação desativado, fluxo sem cookies, formulário sem CSS,
  envio/refresh no desktop, paste Unicode e contraste.
- Build otimizado da imagem final: passou. Smoke tests sem rede passaram para
  `--version`, `--check-config`, UID 10001 e gravação em `/data`; configuração sem
  senha foi rejeitada como esperado. Node e Chromium ausentes na imagem final.

## Limites e operação

O HTTP do protocolo possui DNS/conexão de 5 s, headers/inatividade de leitura e
escrita de 10 s, corpo/global de 30 s. Transferências mais lentas passam a falhar
explicitamente. O runtime tem 2 s adicionais após as etapas de encerramento,
com orçamento total de 37 s versus os 40 s do Compose.

Não foram executados pareamento, envio real, teste em Nokia físico, merge ou
deploy. A validação offline não certifica compatibilidade permanente com o
protocolo remoto. O ponto adicional de recibos de DM continua fora deste conjunto:
não havia falha remota comprovada e seu comportamento não foi alterado.
