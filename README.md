# WhatsApp Dumbphone Gateway — Rust nativo

Gateway pessoal para ler conversas e enviar texto pelo navegador de um celular
simples. Backend Rust com Axum/Tokio e integração de protocolo
[whatsapp-rust](https://github.com/oxidezap/whatsapp-rust), **sem Node, Chromium ou
WebView**. O Nokia acessa HTML; o binário roda em um PC ou servidor ligado.

Esta reescrita precisa de validação de pareamento, histórico e envio com uma
conta de teste antes de uso diário. Testes automatizados usam um gateway falso;
eles não comprovam compatibilidade com o serviço nem com um Nokia físico.

## Funcionalidades

- Layout responsivo com lista lateral no desktop, conversa em uma coluna no celular
  e modo compacto para 128–240 px, sem JavaScript obrigatório.
- Até 20 conversas e as 10 mensagens mais recentes por conversa, obtidas do
  histórico local sincronizado. Identificação de remetentes e indicadores de mídia.
- Login por senha, sessões persistentes de 30 dias e bookmark opcional sem cookies.
- CSRF com expiração de uma hora, escape de HTML, limites de corpo/mensagem,
  limitação de tentativas e deduplicação persistente por formulário.
- Histórico e credenciais de dispositivo em SQLite; admissão HTTP e envios limitados.
- Exclusão de instâncias por diretório e encerramento em SIGINT/SIGTERM.

Chamadas, download/reprodução de mídia e envio de anexos não fazem parte desta
interface. Horários são mostrados em UTC. Mensagens de texto visíveis são limitadas
a 4096 caracteres Unicode por bolha para manter a página pequena. A sincronização recebida
do telefone determina quanto histórico estará disponível; não é um arquivo
completo de todas as mensagens da conta.

## Layout responsivo e modo compacto

A interface usa fontes do sistema, CSS local embutido e ícones SVG pequenos.
Não carrega bibliotecas de UI, fontes, imagens ou scripts externos. Em telas a
partir de 860 px, a lista lateral e a conversa ficam lado a lado; em celulares,
a conversa ocupa uma coluna com link para voltar à lista. Envio e atualização
continuam por formulários/links: não há atualização automática, indicador de
presença ou confirmação de leitura inventada pela interface.

O link **Compact view** ativa `view=compact`. Por exemplo:
`https://seu-dominio/?token=SEU_TOKEN&view=compact`. Esse modo omite o CSS de telas
maiores e a consulta/lista lateral na página da conversa. A escolha acompanha
login, navegação, atualização e redirecionamento após envio, inclusive sem cookies
quando o bookmark legado está habilitado. **Responsive view** retorna ao layout
adaptativo. Após enviar ou atualizar uma conversa, uma âncora HTML mostra o fim do
histórico, inclusive sem JavaScript. O campo não trunca colagens: o servidor valida
o limite de 4096 caracteres Unicode e rejeita mensagens maiores.
Telas estreitas também recebem a base simples no modo automático;
o modo explícito reduz o HTML transferido para aparelhos antigos.

Testes em Chromium verificam larguras de 128 a 1440 px, mas não substituem a
validação em um Nokia físico nem comprovam suporte a TLS no aparelho.

Para revisar o visual sem conta, QR ou envio real, execute em um terminal:

```sh
cargo run --locked --example preview
```

A prévia escuta **somente localhost:3080**, usa conversas fictícias em memória,
credenciais públicas de teste e banco temporário. Não a exponha como servidor de
produção. O terminal mostra a URL de acesso; a senha de teste é
`preview-password-only`. O binário principal não inclui essa prévia.

Em outro terminal, com Python Playwright e seu Chromium instalados:

```sh
python3 scripts/check_layout.py
```

Para usar Chromium do sistema, defina `PLAYWRIGHT_CHROMIUM_EXECUTABLE` com o
caminho do executável. O teste produz capturas em `/tmp/gateway-layout` (ou no
caminho passado por `--artifacts`) e verifica navegação, envio sem JavaScript e
sem cookies, formulário sem CSS, mudança de modo e login. Use uma prévia recém
iniciada para dados de teste consistentes; nunca aponte para uma conta real.

## Instalação com Docker

```sh
git clone https://github.com/vivanjoao/Whatsapp-Dumbphone.git
cd Whatsapp-Dumbphone
git switch rewrite/native-rust
cp .env.example .env
```

Edite `.env`: defina `PASSWORD` único com pelo menos 12 caracteres e gere
`SESSION_SECRET` com `openssl rand -hex 32`. Não use os placeholders de exemplo.
Deixe `ACCESS_TOKEN` vazio para desabilitar bookmarks ou gere com
`openssl rand -hex 16`. Proteja o arquivo: `chmod 600 .env` no Linux/macOS.

```sh
docker compose up -d --build
docker compose logs -f
```

Escaneie o QR exibido no **terminal administrativo** usando WhatsApp → Dispositivos
conectados no smartphone principal. Não publique esses logs. O volume
`native-data` preserva credenciais e histórico. `start.sh`/`start.bat` executam
esse fluxo depois de o `.env` ter sido configurado.

O Compose publica HTTP apenas em `127.0.0.1:3000`. Configure um proxy HTTPS para
acessar remotamente. Para parar: `docker compose down`. **Não use `down -v`** se
quiser preservar credenciais e histórico.

## Execução nativa e desenvolvimento

Rust 1.99.1 (fixado em `rust-toolchain.toml`) e toolchain C para SQLite/ring.
Não é necessário instalar Node, Chromium, SQLite de sistema ou protoc.

```sh
cargo build --release --locked
./target/release/dumbphone-gateway --check-config
./target/release/dumbphone-gateway
```

O binário carrega `.env` do diretório atual. O container tem Linux como alvo;
compilação/execução em Windows e macOS precisa de validação nesses sistemas.

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

O protocolo e o chat-store estão fixados na mesma revisão Git, e `Cargo.lock`
fixa as dependências transitivas. A feature SIMD que exige nightly está desativada.
O CI executa formatação, lint, testes e build do container.

## Configuração

| Variável | Uso |
| --- | --- |
| `PASSWORD` | Senha obrigatória, mínimo 12 caracteres e máximo 1024 unidades UTF-16 |
| `SESSION_SECRET` | Segredo obrigatório, mínimo 32 caracteres aleatórios |
| `ACCESS_TOKEN` | Vazio ou token hexadecimal com 32–128 caracteres |
| `LEGACY_TOKEN_URLS` | `true` preserva navegação sem cookies; `false` troca bookmark por cookie |
| `COOKIE_SECURE` | `true` para o acesso HTTPS; padrão `false` para testes locais |
| `BIND_ADDR` | Endereço HTTP, padrão `0.0.0.0:3000` |
| `DATA_DIR` | Dados persistentes, padrão `./data`; Compose força `/data` |
| `MAX_REQUESTS` | Requisições simultâneas, padrão 64, máximo 1024 |
| `MAX_SENDS` | Envios simultâneos, padrão 4, máximo 32 |

`TRUST_PROXY`, `AUTH_DIR` e `PUPPETEER_EXECUTABLE_PATH` da versão Node não são
usados. O servidor não interpreta cabeçalhos de IP encaminhados. O limite de
login é de 5 tentativas em 15 minutos por IP de conexão, persistido em SQLite;
atrás de um proxy as tentativas compartilham o IP desse proxy. Configure limites
adicionais no proxy se houver vários usuários.

Os formulários de login em várias abas reutilizam o contexto pré-login válido;
autenticar regenera a sessão. Assinaturas de formulário têm representação
canônica, para que mudanças de maiúsculas/minúsculas não contornem a deduplicação.

## Nokia, cookies e HTTPS

No Nokia 2660 Flip 4G, abra o domínio HTTPS no navegador integrado. Valide abertura,
certificado, teclado, acentos e envio no aparelho: o suporte depende da versão do
navegador/firmware. Não reduza a segurança pública para contornar TLS incompatível.

Bookmark opcional: `https://seu-dominio/?token=SEU_TOKEN`. Com
`LEGACY_TOKEN_URLS=true`, o token segue em links e campos de formulário, sem exigir
cookies. É uma credencial: histórico do navegador e logs de acesso do proxy podem
retê-lo. Configure o proxy para omitir query strings/corpos de pedido e nunca
compartilhe essa URL. Respostas usam `no-store` e `no-referrer`, mas isso não apaga
histórico nem controla logs externos.

Com `LEGACY_TOKEN_URLS=false`, o bookmark cria uma sessão por cookie e redireciona
para URL limpa. Login por senha também exige cookies. Cookies são HttpOnly e
SameSite=Lax; `COOKIE_SECURE=true` acrescenta Secure mesmo atrás de proxy.

O servidor **não termina TLS**. Use Caddy/Nginx ou túnel com HTTPS, sem desafios
que exijam JavaScript no Nokia. HTTP público expõe credenciais e mensagens.

## Persistência, recuperação e limites de escala

`DATA_DIR` contém:

- `gateway.lock`: lock de sistema operacional; não apagar para forçar acesso.
- `whatsapp.db` e WAL: identidade, chaves de protocolo, contatos e histórico.
- `web.db` e WAL: hashes de sessões, limites de login e IDs/estados de submissões.

O diretório usa permissão 0700 em Unix; o container roda como UID/GID 10001.
Os bancos **não têm criptografia de disco**. Use armazenamento criptografado e
proteja backups. Pare a instância antes de copiar os bancos, incluindo WAL, ou
use um mecanismo de backup SQLite consistente. Sessões expiradas e IDs de
submissões antigos são removidos periodicamente; não há exclusão automática do
histórico WhatsApp. Monitore disco e estabeleça retenção antes de uso prolongado.

Trocar `PASSWORD`, `SESSION_SECRET` ou `ACCESS_TOKEN` e reiniciar invalida as sessões
e formulários anteriores. Remover `ACCESS_TOKEN` revoga bookmarks. Dados de sessão
web contêm hashes de identificadores, sem senha nem conteúdo de mensagem.

**Uma instância dona de uma conta por DATA_DIR**, em filesystem local. Não escale a
mesma conta com réplicas compartilhando SQLite/NFS. Várias contas exigem diretórios,
processos e portas separados. A arquitetura escala a concorrência HTTP dentro de
limites e permite separar contas; não oferece distribuição automática de contas.
SQLite usa WAL e sincronização FULL; o protocolo tem um escritor e dois leitores.
Os pools web e WhatsApp são distintos; não há uma consulta de rede por listagem.
A lista resolve nomes pelo contato quando necessário e pagina o histórico local
para encontrar até 20 conversas suportadas. Newsletters e status não são oferecidos
como links, pois esta interface atende contatos, LIDs e grupos. Abrir a conversa
também limpa a marcação manual de não lido.

O recebimento normal usa um hook de persistência antes do ACK, com deduplicação de
mensagens. Eventos auxiliares/histórico usam mailbox ordenado de 128 eventos. Se
ela transbordar ou o armazenamento falhar, a prontidão fica indisponível em vez de
servir uma visão aparentemente íntegra. Investigue recursos e reinicie; recuperação
completa de metadados depende de nova sincronização. Isso não é uma garantia de
entrega de todos os tipos de evento sob carga arbitrária.

`/health/live` indica HTTP ativo; `/health/ready` retorna 503 até conexão/autenticação
e também em caso de falha de materialização/overflow. Requisições que excedem
admissão retornam 503. O prazo HTTP é 30 s; envios têm prazo de 20 s. Um formulário
só pode ser submetido uma vez, inclusive após reinício. Falha/timeout é ambíguo:
verifique a conversa antes de abrir novo formulário e enviar de novo. Não existe
reenvio automático de submissões pendentes ou incertas.

O cliente HTTP do protocolo possui prazos dentro das operações bloqueantes:
DNS/conexão de 5 s, espera de headers e inatividade de leitura/escrita de 10 s,
corpo e requisição completa de 30 s. Downloads que ultrapassarem esses limites
falham, inclusive durante sincronização; não são concluídos parcialmente como
se tivessem sucesso. Em redes lentas, investigue a conectividade antes de tentar
novo pareamento.

O encerramento reserva até 25 s para HTTP, 5 s para o worker WhatsApp, 5 s para
o histórico e 2 s adicionais para o runtime, abaixo dos 40 s do Compose. Um worker
bloqueante que exceda o prazo não faz o runtime esperar indefinidamente. O lock
da conta permanece retido até a saída do processo. Encerramento incompleto retorna
erro; isso não substitui backup nem garante que toda operação incerta foi concluída.

Não há números comparativos de RAM/CPU/latência nesta entrega. A remoção de Node e
Chromium é verificável no build; o ganho real depende do histórico, conta e carga.

## Migração da versão Node

1. Faça backup do projeto/`.env` e preserve o antigo diretório `session`.
2. Pare a versão Node. Não conecte duas instâncias ao mesmo perfil.
3. Use esta branch e o novo volume `native-data`, sem apagar o volume antigo.
4. Revise `.env`, compile e faça **novo pareamento**: perfis Chromium/LocalAuth não
   são compatíveis com a biblioteca nativa. Sessões web antigas também não migram.
5. Espere a sincronização, valide conversas/grupos e teste envio em conversa
   autorizada. Histórico antigo pode não ser integralmente transferido.
6. Para voltar, pare esta instância e use `fix/p0-p1-security` com o perfil antigo;
   talvez seja necessário parear novamente. Não remova os dados de nenhuma versão
   até concluir a validação.

`AGENTS.md` contém as regras versionadas. A decisão técnica e limitações estão em
[docs/decisions/001-native-rust.md](docs/decisions/001-native-rust.md).

## Integração não oficial

A biblioteca é uma implementação não oficial do protocolo. Mudanças do WhatsApp
podem exigir atualização; a migração não torna o serviço uma API oficial. Consulte
as condições e limitações do [projeto upstream](https://github.com/oxidezap/whatsapp-rust)
antes de vincular uma conta importante.
