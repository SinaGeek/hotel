from rest_framework import viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from apps.stays.models import Stay
from .serializers import StaySerializer
from apps.stays.services.stay_service import StayService
class StayViewSet(viewsets.ReadOnlyModelViewSet):
    queryset=Stay.objects.all(); serializer_class=StaySerializer
    def get_queryset(self): return self.queryset.filter(hotel_id=self.request.hotel_id)
    @action(detail=False, methods=['post'])
    def check_in(self,request):
        from apps.reservations.models import Reservation
        r=Reservation.objects.get(id=request.data['reservation_id'],hotel_id=request.hotel_id)
        stay=StayService.check_in(r)
        return Response(StaySerializer(stay).data)
    @action(detail=True, methods=['post'])
    def check_out(self,request,pk=None):
        stay=self.get_object(); StayService.check_out(stay); return Response({'status':'ok'})
