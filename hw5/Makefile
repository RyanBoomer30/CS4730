# docker build all 3 images
build:
	docker build . -f BootstrapDockerfile -t prj5-bootstrap --progress=plain --no-cache
	docker build . -f PeerDockerfile -t prj5-peer 
	docker build . -f ClientDockerfile -t prj5-client

build1:
	docker build . -f BootstrapDockerfile -t prj5-bootstrap --progress=plain --no-cache

build2:
	docker build . -f PeerDockerfile -t prj5-peer

build3:
	docker build . -f ClientDockerfile -t prj5-client

clean:
	docker rmi prj5-bootstrap
	docker rmi prj5-peer
	docker rmi prj5-client

up1: 
	docker compose -f docker-compose-testcase-1.yml up

up2: 
	docker compose -f docker-compose-testcase-2.yml up

up3: 
	docker compose -f docker-compose-testcase-3.yml up

up4: 
	docker compose -f docker-compose-testcase-4.yml up

up5: 
	docker compose -f docker-compose-testcase-5.yml up