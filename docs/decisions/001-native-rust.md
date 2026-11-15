# 001 — Gateway nativo com dono único da conta

Status: implementado em branch para revisão; pareamento real e teste no Nokia pendentes.

O proprietário autorizou a reescrita para código nativo e escalável após discutir
os custos de Node/Chromium. Trocar apenas Express não eliminaria o navegador.
Escolhemos Rust/Axum/Tokio e uma implementação de protocolo sem browser.

`whatsapp-rust` e seu `chat-store` estão fixados em
`f8165f282008935732e0b26d6f5cca5038ecd222` (release 0.7.0). A API do chat-store
não foi publicada como estável; a revisão fixa evita deriva silenciosa. A versão
mínima declarada pelo upstream é 1.94, mas o toolchain usado/CI fica fixado em 1.99.1.

## Fronteiras

- Domínio e contrato Gateway independentes de Axum e da biblioteca de protocolo.
- HTTP valida requisição, autentica e controla admissão; views só renderizam dados.
- Adaptador normaliza JIDs, resolve nomes e consulta histórico SQLite materializado.
- O mesmo arquivo SQLite do upstream guarda identidade e histórico com pool/WAL
  compartilhado entre os dois componentes do protocolo, com um escritor, dois
  leitores e sincronização FULL.
- SQLite web separado, com pool de quatro conexões; operações bloqueantes saem do
  executor assíncrono. Somente hashes dos identificadores de sessão são persistidos.

## Concorrência e consistência

- Lock exclusivo de processo por DATA_DIR; nunca apagar locks para forçar startup.
- 64 requisições / 4 envios em andamento por padrão. Rejeitar excesso, sem fila HTTP
  ilimitada. Limites configuráveis e testes de admissão.
- Persistir reserva de formulário antes do efeito externo. Estado sent/unknown;
  duplicatas retornam 409. Pendências no reinício tornam-se unknown, sem replay.
- Recebimento de mensagens passa por hook serializado e gravação durável antes do
  ACK; replay do SDK deduplica mensagens no chat-store. Não equivale a exactly-once
  de rede nem impede duplicata feita com um formulário novo.
- Eventos auxiliares usam fila ordenada limitada. Overflow detectado pelas
  estatísticas do SDK e falhas de armazenamento deixam readiness em 503. Não há
  promessa de recuperação integral de metadados ou controle de carga no servidor remoto.

## Alternativas e limites

- Rust + processo Node manteria Chromium e aumentaria fronteiras operacionais.
- A API oficial de negócios não é substituta direta para acompanhar conversas
  pessoais existentes.
- Não desenvolver criptografia/protocolo próprio: biblioteca externa fixa e isolada.
- Uma conta tem um dono, não réplicas competindo pela sessão. Para várias contas,
  isolar processos/dados; orquestração e autenticação multiusuário ficam fora do escopo.
- Histórico exige retenção/backup e teste de migração. A sessão Chromium não migra.
- TLS termina em proxy; compatibilidade de certificado/browser é validada no Nokia.
- Ganhos de desempenho precisam de benchmark comparável com a mesma carga e conta;
  não inferir uma porcentagem apenas da escolha da linguagem.

## Validação antes de produção

Suíte offline de segurança/HTTP/persistência/concorrência; lint e formatação;
build nativo/container. Depois, com autorização específica: parear conta de teste,
validar restauração, sincronização inicial, PN/LID, grupos, receipts, desconexão,
envio ambíguo e uso no Nokia 2660 Flip 4G por HTTPS. Não usar a conta real como
ensaio automático.
