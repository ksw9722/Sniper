# Fuzzer 설정 보조 기능 사용법

## 사용 순서

1. HTTP History나 HTTP Replay에서 요청을 골라 `Send to Fuzzer`를 누른다. 다른 요청을 가져와도 준비한 payload 목록은 남는다.
2. 요청에서 값을 선택하고 `Mark selection`을 누른다. 요청 위 `Value`에서 query/form 값을 골라 `Mark value`를 눌러도 된다. 이름이 같은 파라미터는 따로 표시되고, 인코딩은 바뀌지 않는다. `Undo marker`는 마지막 표시를 되돌린다. 표시한 뒤 요청을 고쳤다면 되돌리지 않는다.
3. Payload를 한 줄에 하나씩 붙여 넣는다. UTF-8 텍스트 파일, 작은 경계값 preset, 정수 범위도 쓸 수 있다. 기본으로는 기존 목록에 더한다. `Replace list`를 고르면 기존 목록을 바꾼다.
4. 상단에서 실제 연결 대상, HTTP 버전, marker 수, `N payloads → N planned requests`를 확인한다. marker가 여러 개면 요청마다 모든 marker에 같은 payload가 들어간다. 위치별 조합은 만들지 않는다.
5. `Preview & details`에서 샘플을 최대 3개 본다. Content-Length는 본문 길이에 맞춰 다시 계산한다. 샘플은 템플릿을 확장한 미리보기다. 실제 전송 내용은 Match/Replace 등으로 달라질 수 있다. 큰 샘플은 생략하거나, 잘렸다는 안내를 보여 준다.
6. 대상이 본인 소유이거나 테스트 허가를 받았는지 확인하고 `Start`를 누른다. 요청은 차례로 보내고, 모두 끝나면 결과를 돌려준다. 실행 중 취소와 실시간 진행률은 없다.
7. 결과를 상태 코드 그룹, 크기(응답 본문 bytes), 소요 시간으로 거르고 정렬한다. 결과를 골라 요청과 응답을 본다. 두 결과를 비교하려면 한쪽에서 `Set compare base`를, 다른 쪽에서 `Compare selected`를 누른다. 기존 비교 화면이 열린다.

## 빈 값과 파일

- 공백을 지우지 않는다. `$&`, `$1`, `$payload$` 같은 문자열도 글자 그대로 쓴다.
- 빈 줄은 빈 payload다. 마지막 줄바꿈 하나는 구분자로만 본다. 빈 payload 하나만 넣으려면 빈 줄 하나를 입력한다.
- 파일의 CRLF와 CR은 붙여 넣을 때처럼 줄바꿈으로 처리한다. UTF-8 BOM은 문자로 남긴다. 잘못된 UTF-8은 변환하지 않고 거절한다.
- 파일을 읽는 사이 요청, 목록, 세션이 바뀌면 그 읽기 결과는 적용하지 않는다.
- 본문 값 선택은 완전한 UTF-8 본문에서만 돕는다. Base64, 바이너리, 잘린 본문을 일반 form 값처럼 바꾸지 않는다. Base64 본문에 marker를 직접 넣는 일부 API 기능은 이 화면에서 쓸 수 없다.

## 저장된 실행 다시 보기

`View or reuse a saved run`에서 `Refresh`를 누르고 실행을 고른다.

- `View results`: 현재 요청, payload, 연결 대상을 그대로 두고 저장된 결과만 본다.
- `Reuse setup`: 확인을 받은 뒤 요청과 payload 초안을 바꾼다. 저장된 실행에는 원래 원문 포맷, source transaction, target override가 없어 복구하지 않는다. 현재 초안에 있던 무관한 source와 target도 지운다. HTTP 버전은 저장된 template 버전이 있으면 그 값을, 없으면 HTTP/1.1을 쓴다. 실행 전에 대상과 버전을 다시 확인한다.
- 줄바꿈이 든 API payload나 편집기 한도를 넘는 실행은 초안으로 복원하지 않는다. `View results`로는 볼 수 있다.

## 한도

- Payload 최대 5,000개, 각 64 KiB, 바이트 합계 4 MiB
- Payload 편집기 텍스트와 가져올 파일은 각각 4 MiB 이하. 편집기 한도는 줄 구분자까지 세므로 바이트 합계 한도와 다르다.
- 요청 편집기 텍스트 2 MiB, marker 최대 128개, payload별 확장 요청 4 MiB
- 서버는 보내기 전에 확장 요청을 모두 검증한다. 계획한 요청 수가 전송에 성공한 수는 아니다.
