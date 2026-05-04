from rest_framework import viewsets, status
from rest_framework.response import Response
from apps.reservations.models import Reservation
from .serializers import ReservationSerializer
from apps.reservations.services.reservation_service import ReservationService
class ReservationViewSet(viewsets.ModelViewSet):
    serializer_class=ReservationSerializer
    queryset=Reservation.objects.all()
    def get_queryset(self): return self.queryset.filter(hotel_id=self.request.hotel_id)
    def create(self, request,*args,**kwargs):
        s=self.get_serializer(data=request.data); s.is_valid(raise_exception=True)
        obj=ReservationService.create(**s.validated_data)
        return Response(self.get_serializer(obj).data,status=status.HTTP_201_CREATED)
